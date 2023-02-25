use std::{
	collections::HashMap,
	pin::Pin,
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
use raiden_network_messages::{
	decode::MessageDecoder,
	messages::{
		OutgoingMessage,
		TransportServiceMessage,
	},
};
use raiden_state_machine::types::QueueIdentifier;
use tokio::{
	select,
	sync::mpsc::{
		self,
		UnboundedSender,
	},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::error;

use super::{
	queue::RetryMessageQueue,
	MatrixClient,
};
use crate::config::TransportConfig;

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
			let (queue, sender) = RetryMessageQueue::new(
				queue_identifier.recipient,
				self.sender.clone(),
				self.config.clone(),
			);
			self.running_futures.push(Box::pin(queue.run()));

			self.message_queues.insert(queue_identifier, sender);
		}
	}

	pub async fn run(mut self) {
		let mut sync_settings = SyncSettings::new().timeout(Duration::from_secs(30));
		loop {
			select! {
				response = self.client.sync_once(sync_settings.clone()).fuse() => {
					match response {
						Ok(response) => {
							let to_device_events = response.to_device.events;
							for to_device_event in to_device_events.iter() {
								self.process_event(to_device_event).await;
							}
							let sync_token = response.next_batch;
							sync_settings = SyncSettings::new().timeout(Duration::from_secs(30)).token(sync_token);
						},
						Err(_e) => {

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
						Some(TransportServiceMessage::Send(_message)) => {
							//self.client.send();
						},
						_ => {}
					}
				}
			}
		}
	}

	pub async fn process_event(&self, event: &Raw<AnyToDeviceEvent>) {
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
				let message = match MessageDecoder::decode(content.clone()) {
					Ok(message) => message,
					Err(e) => {
						error!("Could not decode message: {}", message_type);
						return
					},
				};

				println!("Message received: {:?}", message);
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
