use std::{
	collections::HashMap,
	pin::Pin,
};

use futures::{
	stream::FuturesUnordered,
	Future,
	FutureExt,
	StreamExt,
};
use matrix_sdk::ruma::to_device::DeviceIdOrAllDevices;
use raiden_network_messages::messages::{
	self,
	OutgoingMessage,
	TransportServiceMessage,
};
use raiden_primitives::types::{
	MessageIdentifier,
	QueueIdentifier,
};
use raiden_storage::matrix::MatrixStorage;
use raiden_transition::messages::MessageHandler;
use tokio::{
	select,
	signal::unix::{
		signal,
		SignalKind,
	},
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
use crate::{
	config::TransportConfig,
	matrix::{
		queue::QueueOp,
		MessageContent,
		MessageType,
	},
};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + Sync + 'a>>;

struct QueueInfo {
	op_sender: UnboundedSender<QueueOp>,
	messages: HashMap<MessageIdentifier, OutgoingMessage>,
}

impl Into<HashMap<MessageIdentifier, OutgoingMessage>> for QueueInfo {
	fn into(self) -> HashMap<MessageIdentifier, OutgoingMessage> {
		self.messages
	}
}

pub struct MatrixService {
	config: TransportConfig,
	client: MatrixClient,
	matrix_storage: MatrixStorage,
	our_sender: UnboundedSender<TransportServiceMessage>,
	queue_receiver: UnboundedReceiverStream<TransportServiceMessage>,
	messages: HashMap<QueueIdentifier, QueueInfo>,
	running_futures: FuturesUnordered<BoxFuture<'static, ()>>,
}

impl MatrixService {
	pub fn new(
		config: TransportConfig,
		client: MatrixClient,
		matrix_storage: MatrixStorage,
	) -> (Self, UnboundedSender<TransportServiceMessage>) {
		let (sender, receiver) = mpsc::unbounded_channel();

		(
			Self {
				config,
				client,
				matrix_storage,
				messages: HashMap::new(),
				our_sender: sender.clone(),
				queue_receiver: UnboundedReceiverStream::new(receiver),
				running_futures: FuturesUnordered::new(),
			},
			sender,
		)
	}

	pub fn init_from_storage(&mut self) -> Result<(), String> {
		// Get last sync token
		let sync_token = self.matrix_storage.get_sync_token().unwrap_or(String::new());
		if !sync_token.trim().is_empty() {
			self.client.set_sync_token(sync_token);
		}

		let messages: HashMap<QueueIdentifier, HashMap<MessageIdentifier, OutgoingMessage>> = self
			.matrix_storage
			.get_messages()
			.map_err(|e| format!("Error initializing transport from storage: {:?}", e))
			.and_then(|data| {
				if !data.is_empty() {
					serde_json::from_str(&data)
						.map_err(|e| format!("Error initializing transport from storage: {:?}", e))
				} else {
					Ok(HashMap::new())
				}
			})?;

		for (queue_identifier, messages) in messages.iter() {
			self.ensure_message_queue(queue_identifier.clone(), messages.clone());
			let queue_info =
				self.messages.get(queue_identifier).expect("Should already be crealted");
			for message in messages.values() {
				let _ = queue_info.op_sender.send(QueueOp::Enqueue(message.message_identifier));
			}
		}

		Ok(())
	}

	fn ensure_message_queue(
		&mut self,
		queue_identifier: QueueIdentifier,
		messages: HashMap<MessageIdentifier, OutgoingMessage>,
	) {
		if let None = self.messages.get(&queue_identifier) {
			let (queue, sender) =
				RetryMessageQueue::new(self.our_sender.clone(), self.config.clone());
			self.running_futures.push(Box::pin(queue.run()));

			self.messages
				.entry(queue_identifier)
				.or_insert(QueueInfo { op_sender: sender, messages });
		}
	}

	pub async fn run(mut self, message_handler: MessageHandler) {
		let mut hangup = match signal(SignalKind::interrupt()) {
			Ok(s) => s,
			Err(e) => {
				eprintln!("Could not instantiate listener for hangup signal: {:?}", e);
				return
			},
		};
		loop {
			select! {
				_ = hangup.recv().fuse() => {
					let messages: HashMap<QueueIdentifier, HashMap<MessageIdentifier, OutgoingMessage>> = self.messages.into_iter().map(|(queue_identifier, queue_info)| (queue_identifier, queue_info.messages)).collect();
					let messages_data = match serde_json::to_string(&messages) {
						Ok(data) => data,
						Err(e) => {
							error!("Could not serialize messages for storage: {:?}", e);
							return
						}
					};
					if let Err(e) = self.matrix_storage.set_sync_token(self.client.get_sync_token()) {
						error!("Could not store matrix sync token: {:?}", e);
					}
					if let Err(e) = self.matrix_storage.store_messages(messages_data) {
						error!("Could not store matrix messages: {:?}", e);
					}
					return
				},
				() = self.running_futures.select_next_some(), if self.running_futures.len() > 0 => {},
				messages = self.client.get_new_messages().fuse() => {
					let messages = match messages {
						Ok(messages) => messages,
						Err(e) => {
							error!("Sync error: {:?}", e);
							continue;
						}
					};

					for message in messages {
						let _ = message_handler.handle(message).await;
					}
				},
				message = self.queue_receiver.next() => {
					match message {
						Some(TransportServiceMessage::Enqueue((queue_identifier, message))) => {
							self.ensure_message_queue(queue_identifier.clone(), HashMap::new());
							let queue = self.messages
								.get_mut(&queue_identifier)
								.expect("Queue should have been created before.");
							let _ = queue
								.op_sender
								.send(QueueOp::Enqueue(message.message_identifier));
							queue.messages.insert(message.message_identifier, message);
						},
						Some(TransportServiceMessage::Dequeue((queue_identifier, message_identifier))) => {
							if let Some(queue_identifier) = queue_identifier {
								if let Some(queue_info) = self.messages.get_mut(&queue_identifier) {
									let _ = queue_info.op_sender.send(QueueOp::Dequeue(message_identifier));
									queue_info.messages.retain(|_, msg| msg.message_identifier != message_identifier);
								}
							} else {
								for queue_info in self.messages.values_mut() {
									let _ = queue_info.op_sender.send(QueueOp::Dequeue(message_identifier));
									queue_info.messages.retain(|_, msg| msg.message_identifier != message_identifier);
								}
							}
						},
						Some(TransportServiceMessage::Send(message_identifier)) => {
							let message_by_identifier = self.messages
								.values()
								.find_map(|queue_info| {
									queue_info
										.messages
										.values()
										.find(|m| m.message_identifier == message_identifier)
								})
								.cloned();
							if let Some(message) = message_by_identifier {
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
							}
						},
						Some(TransportServiceMessage::Broadcast(message)) => {
							let (message_json, device_id) = match message.inner {
								messages::MessageInner::PFSCapacityUpdate(inner) => {
									let message_json = match serde_json::to_string(&inner) {
										Ok(json) => json,
										Err(e) => {
											error!("Could not serialize message: {:?}", e);
											continue;
										}
									};
									(message_json, "PATH_FINDING")
								},
								messages::MessageInner::PFSFeeUpdate(inner) => {
									let message_json = match serde_json::to_string(&inner) {
										Ok(json) => json,
										Err(e) => {
											error!("Could not serialize message: {:?}", e);
											continue;
										}
									};
									(message_json, "PATH_FINDING")
								},
								_ => {
									// No other messages should be broadcasted
									return
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
							if let Err(e) = self.client.broadcast(json, DeviceIdOrAllDevices::DeviceId(device_id.into())).await {
								error!("Could not broadcast message {:?}", e);
							};
						}
						_ => {}
					}
				}
			}
		}
	}
}
