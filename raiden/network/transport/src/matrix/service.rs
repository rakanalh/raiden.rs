use std::{
	collections::HashMap,
	pin::Pin,
	sync::Arc,
	time::Duration,
};

use futures::{
	stream::FuturesUnordered,
	Future,
	FutureExt,
	StreamExt,
};
use matrix_sdk::{
	config::SyncSettings,
	ruma::{
		events::AnyToDeviceEvent,
		serde::Raw,
	},
};
use parking_lot::RwLock;
use raiden_network_messages::{
	decode::MessageDecoder,
	messages::{
		OutgoingMessage,
		TransportServiceMessage,
	},
};
use raiden_primitives::types::QueueIdentifier;
use raiden_transition::{
	manager::StateManager,
	Transitioner,
};
use tokio::{
	select,
	sync::mpsc::{
		self,
		UnboundedSender,
	},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{
	debug,
	error,
	info,
};

use super::{
	queue::RetryMessageQueue,
	MatrixClient,
};
use crate::{
	config::TransportConfig,
	matrix::{
		MessageContent,
		MessageType,
	},
};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + Sync + 'a>>;

pub struct MatrixService {
	config: TransportConfig,
	client: MatrixClient,
	sender: UnboundedSender<TransportServiceMessage>,
	receiver: UnboundedReceiverStream<TransportServiceMessage>,
	message_queues: HashMap<QueueIdentifier, UnboundedSender<(QueueIdentifier, OutgoingMessage)>>,
	running_futures: FuturesUnordered<BoxFuture<'static, ()>>,
}

impl MatrixService {
	pub fn new(
		config: TransportConfig,
		client: MatrixClient,
	) -> (Self, UnboundedSender<TransportServiceMessage>) {
		let (sender, receiver) = mpsc::unbounded_channel();

		(
			Self {
				config,
				client,
				sender: sender.clone(),
				receiver: UnboundedReceiverStream::new(receiver),
				message_queues: HashMap::new(),
				running_futures: FuturesUnordered::new(),
			},
			sender,
		)
	}

	async fn create_message_queue_if_not_exists(&mut self, queue_identifier: QueueIdentifier) {
		if let None = self.message_queues.get(&queue_identifier) {
			let (queue, sender) = RetryMessageQueue::new(self.sender.clone(), self.config.clone());
			self.running_futures.push(Box::pin(queue.run()));

			self.message_queues.insert(queue_identifier, sender);
		}
	}

	pub async fn run(
		mut self,
		state_manager: Arc<RwLock<StateManager>>,
		transition_service: Arc<Transitioner>,
		message_decoder: MessageDecoder,
	) {
		let mut sync_settings = SyncSettings::new().timeout(Duration::from_secs(30));
		loop {
			select! {
				() = self.running_futures.select_next_some(), if self.running_futures.len() > 0 => {},
				response = self.client.sync_once(sync_settings.clone()).fuse() => {
					match response {
						Ok(response) => {
							let to_device_events = response.to_device.events;
							info!("Received {} network messages", to_device_events.len());
							for to_device_event in to_device_events.iter() {
								self.process_event(state_manager.clone(), transition_service.clone(), message_decoder.clone(), to_device_event).await;
							}
							let sync_token = response.next_batch;
							sync_settings = SyncSettings::new().timeout(Duration::from_secs(30)).token(sync_token);
						},
						Err(e) => {
							error!("Sync error: {:?}", e);
						}
					}
				},
				message = self.receiver.next() => {
					match message {
						Some(TransportServiceMessage::Enqueue((queue_identifier, message))) => {
							self.create_message_queue_if_not_exists(queue_identifier.clone()).await;
							let _ = self.message_queues
								.get(&queue_identifier)
								.expect("Queue should have been created before.")
								.send((queue_identifier, message));
						},
						Some(TransportServiceMessage::Send(message)) => {
							let message_json = match serde_json::to_string(&message) {
								Ok(json) => json,
								Err(e) => {
									error!("Could not serialize message: {:?}", e);
									continue;
								}
							};
							let content = MessageContent { msgtype: MessageType::Text.to_string(), body: message_json };
							let json = match serde_json::to_string(&content) {
								Ok(json) => json,
								Err(e) => {
									error!("Could not serialize message: {:?}", e);
									continue;
								}
							};
							if let Err(e) = self.client.send(json, message.recipient_metadata).await {
								error!("Could not send message {:?}", e);
							};
						},
						_ => {}
					}
				}
			}
		}
	}

	pub async fn process_event(
		&self,
		state_manager: Arc<RwLock<StateManager>>,
		transitioner: Arc<Transitioner>,
		message_decoder: MessageDecoder,
		event: &Raw<AnyToDeviceEvent>,
	) {
		match event.get_field::<String>("type") {
			Ok(Some(message_type)) => {
				let event_body = event.json().get();

				let map: HashMap<String, serde_json::Value> = match serde_json::from_str(event_body)
				{
					Ok(map) => map,
					Err(e) => {
						error!("Could not parse message {}: {}", message_type, e);
						return
					},
				};

				let content = match map.get("content").map(|obj| obj.get("body")).flatten() {
					Some(value) => value,
					None => {
						error!("Message {} has no body: {:?}", message_type, map);
						return
					},
				};

				let s = content.as_str().unwrap().to_owned();
				for line in s.lines() {
					let map: HashMap<String, serde_json::Value> = match serde_json::from_str(&line)
					{
						Ok(map) => map,
						Err(e) => {
							error!("Cannot parse JSON: {:?}\n{}", e, line);
							continue
						},
					};

					let message_type = match map.get("type").map(|v| v.as_str()).flatten() {
						Some(message_type) => message_type,
						None => {
							error!("Cannot find type field: {}", line);
							continue
						},
					};

					let chain_state = state_manager.read().current_state.clone();
					let state_changes = match message_decoder
						.decode(chain_state, line.to_string())
						.await
					{
						Ok(message) => message,
						Err(e) => {
							error!("Could not decode message({}): {} {}", message_type, e, line);
							return
						},
					};

					for state_change in state_changes {
						debug!("Transition state change: {:?}", state_change);
						transitioner.transition(state_change).await;
					}
				}
			},
			Ok(None) => {
				error!("Invalid event. Field 'type' does not exist");
			},
			Err(e) => {
				error!("Invalid event: {:?}", e);
			},
		};
	}
}
