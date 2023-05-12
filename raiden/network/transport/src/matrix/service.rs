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
	IncomingMessage,
	MessageInner,
	OutgoingMessage,
	SignedMessage,
	TransportServiceMessage,
};
use raiden_primitives::{
	constants::CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
	signing,
	traits::Checksum,
	types::{
		MessageIdentifier,
		QueueIdentifier,
	},
};
use raiden_transition::messages::MessageHandler;
use serde::{
	Deserialize,
	Serialize,
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
	error,
	trace,
};

use super::{
	queue::RetryMessageQueue,
	storage::MatrixStorage,
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

#[derive(Serialize, Deserialize)]
struct StorageMessages {
	messages: HashMap<String, HashMap<MessageIdentifier, OutgoingMessage>>,
}

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

		let storage_messages: HashMap<
			QueueIdentifier,
			HashMap<MessageIdentifier, OutgoingMessage>,
		> = self
			.matrix_storage
			.get_messages()
			.map_err(|e| format!("Error initializing transport from storage: {:?}", e))
			.and_then(|data| {
				if !data.is_empty() {
					let data: HashMap<String, HashMap<MessageIdentifier, OutgoingMessage>> =
						serde_json::from_str(&data).map_err(|e| {
							format!("Error initializing transport from storage: {:?}", e)
						})?;
					Ok(data
						.into_iter()
						.map(|(k, v)| (serde_json::from_str(&k).expect("Should deserialize"), v))
						.collect())
				} else {
					Ok(HashMap::new())
				}
			})?;

		for (queue_identifier, messages) in storage_messages.iter() {
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

	pub async fn run(mut self, mut message_handler: MessageHandler) {
		loop {
			select! {
				() = self.running_futures.select_next_some(), if self.running_futures.len() > 0 => {},
				messages = self.client.get_new_messages().fuse() => {
					if let Err(e) = self.matrix_storage.set_sync_token(self.client.get_sync_token()) {
						error!("Could not store matrix sync token: {:?}", e);
					}

					let messages = match messages {
						Ok(messages) => messages,
						Err(e) => {
							error!("Sync error: {:?}", e);
							continue;
						}
					};

					for message in messages {
						if let MessageInner::Processed(_) = message.inner.clone() {
							let queues: Vec<QueueIdentifier> = self.messages.keys().cloned().collect();
							for queue_identifier in queues {
								self.inplace_delete_message_queue(message.clone(), &queue_identifier);
							}
						} else if let MessageInner::Delivered(inner) = message.inner.clone() {
							let sender = match signing::recover(&inner.bytes_to_sign(), &inner.signature.0) {
								Ok(sender) => sender,
								Err(e) => {
									error!("Could not recover address from signature: {}", e);
									continue
								}
							};
							self.inplace_delete_message_queue(message.clone(), &QueueIdentifier {
								recipient: sender,
								canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
							});
						}
						if let Err(e) = message_handler.handle(message).await {
							error!(message = "Error handling message", error = format!("{:?}", e));
						}
					}
				},
				message = self.queue_receiver.next() => {
					match message {
						Some(TransportServiceMessage::Enqueue((queue_identifier, message))) => {
							trace!(
								message = "Enqueue message",
								msg_type = message.type_name(),
								message_identifier = message.message_identifier,
								queue_id = queue_identifier.to_string(),
							);
							self.ensure_message_queue(queue_identifier.clone(), HashMap::new());
							let queue = self.messages
								.get_mut(&queue_identifier)
								.expect("Queue should have been created before.");
							let _ = queue
								.op_sender
								.send(QueueOp::Enqueue(message.message_identifier));
							queue.messages.insert(message.message_identifier, message);
							self.store_messages();
						},
						Some(TransportServiceMessage::Dequeue((queue_identifier, message_identifier))) => {
							if let Some(queue_identifier) = queue_identifier {
								if let Some(queue_info) = self.messages.get_mut(&queue_identifier) {
									trace!(message = "Dequeue message", message_identifier = message_identifier, queue_id = queue_identifier.to_string());
									let _ = queue_info.op_sender.send(QueueOp::Dequeue(message_identifier));
									queue_info.messages.retain(|_, msg| msg.message_identifier != message_identifier);
								}
							} else {
								for (queue_identifier, queue_info) in self.messages.iter_mut() {
									trace!(message = "Dequeue message", message_identifier = message_identifier, queue_id = queue_identifier.to_string());
									let _ = queue_info.op_sender.send(QueueOp::Dequeue(message_identifier));
									queue_info.messages.retain(|_, msg| msg.message_identifier != message_identifier);
								}
							}
							self.store_messages();
						},
						Some(TransportServiceMessage::Send(message_identifier)) => {
							let messages_by_identifier: Vec<OutgoingMessage> = self.messages
								.values()
								.filter_map(|queue_info| {
									Some(queue_info
										.messages
										.values()
										.filter(|m| m.message_identifier == message_identifier)
										.cloned()
										.collect::<Vec<_>>())
								})
								.flatten()
								.collect();
							for message in messages_by_identifier {
								if let MessageInner::Delivered(_) = message.inner.clone() {
									let queue_identifier = &QueueIdentifier {
										recipient: message.recipient,
										canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
									};
									if let Some(queue) = self.messages.get_mut(queue_identifier) {
										queue.messages.remove_entry(&message.message_identifier);
									}
								}
								trace!(
									message = "Sending message",
									message_identifier = message.message_identifier,
									msg_type = message.type_name(),
									recipient = message.recipient.checksum()
								);
								self.send(message).await;
							}
						},
						Some(TransportServiceMessage::Broadcast(message)) => {
							let (message_json, device_id) = match message.inner {
								messages::MessageInner::PFSCapacityUpdate(ref inner) => {
									let message_json = match serde_json::to_string(&inner) {
										Ok(json) => json,
										Err(e) => {
											error!("Could not serialize message: {:?}", e);
											continue;
										}
									};
									(message_json, "PATH_FINDING")
								},
								messages::MessageInner::PFSFeeUpdate(ref inner) => {
									let message_json = match serde_json::to_string(&inner) {
										Ok(json) => json,
										Err(e) => {
											error!("Could not serialize message: {:?}", e);
											continue;
										}
									};
									(message_json, "PATH_FINDING")
								},
								messages::MessageInner::MSUpdate(ref inner) => {
									let message_json = match serde_json::to_string(&inner) {
										Ok(json) => json,
										Err(e) => {
											error!("Could not serialize message: {:?}", e);
											continue;
										}
									};
									(message_json, "MONITORING")
								},
								_ => {
									// No other messages should be broadcasted
									return
								}
							};

							trace!(message = "Broadcast message", msg_type = message.type_name());

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
						},
						Some(TransportServiceMessage::Clear(queue_identifier)) => {
							if let Some(queue_info) = self.messages.get(&queue_identifier) {
								let _ = queue_info.op_sender.send(QueueOp::Stop);
							}
							self.messages.remove(&queue_identifier);
						}
						_ => {}
					}
				}
			}
		}
	}

	async fn send(&self, message: OutgoingMessage) {
		if let Err(e) = self.client.send(message.clone(), message.recipient_metadata).await {
			error!("Could not send message {:?}", e);
		};
	}

	fn store_messages(&self) {
		let messages: HashMap<String, HashMap<MessageIdentifier, OutgoingMessage>> = self
			.messages
			.iter()
			.map(|(queue_identifier, queue_info)| {
				(
					serde_json::to_string(&queue_identifier.clone()).expect("Should serialize"),
					queue_info.messages.clone(),
				)
			})
			.collect();
		let storage_messages = StorageMessages { messages };
		let messages_data = match serde_json::to_string(&storage_messages) {
			Ok(data) => data,
			Err(e) => {
				error!("Could not serialize messages for storage: {:?}", e);
				return
			},
		};
		if let Err(e) = self.matrix_storage.store_messages(messages_data) {
			error!("Could not store messages: {:?}", e);
		}
	}

	/// Check if the message exists in queue with ID `queueid` and exclude if found.
	fn inplace_delete_message_queue(
		&mut self,
		incoming_message: IncomingMessage,
		queue_id: &QueueIdentifier,
	) {
		let queue = match self.messages.get_mut(&queue_id) {
			Some(queue) => queue,
			None => return,
		};

		if queue.messages.is_empty() {
			return
		}

		for (outgoing_message_identifier, outgoing_message) in queue.messages.clone().iter() {
			// A withdraw request is only confirmed by a withdraw confirmation.
			// This is done because Processed is not an indicator that the partner has
			// processed and **accepted** our withdraw request. Receiving
			// `Processed` here would cause the withdraw request to be removed
			// from the queue although the confirmation may have not been sent.
			// This is avoided by waiting for the confirmation before removing
			// the withdraw request.
			if let MessageInner::WithdrawRequest(_) = outgoing_message.inner {
				if !matches!(incoming_message.inner, MessageInner::WithdrawConfirmation(_)) {
					continue
				}
			}

			let incoming_message_identifier = match &incoming_message.inner {
				MessageInner::Delivered(inner) => inner.delivered_message_identifier,
				MessageInner::Processed(inner) => inner.message_identifier,
				MessageInner::SecretRequest(inner) => inner.message_identifier,
				MessageInner::SecretReveal(inner) => inner.message_identifier,
				MessageInner::LockExpired(inner) => inner.message_identifier,
				MessageInner::Unlock(inner) => inner.message_identifier,
				MessageInner::WithdrawRequest(inner) => inner.message_identifier,
				MessageInner::WithdrawConfirmation(inner) => inner.message_identifier,
				MessageInner::WithdrawExpired(inner) => inner.message_identifier,
				_ => 0,
			};
			if outgoing_message_identifier == &incoming_message_identifier {
				trace!(
					message = "Poping message from queue",
					incoming_message = incoming_message.type_name(),
					outgoing_message = outgoing_message.type_name(),
					message_identifier = outgoing_message_identifier,
					queue = queue_id.to_string(),
				);
				queue.messages.remove_entry(&incoming_message_identifier);
			}
		}
	}
}
