use std::{
	cmp::min,
	time::Duration as StdDuration,
};

use chrono::{
	offset::Local,
	DateTime,
	Duration,
};
use futures::StreamExt;
use raiden_network_messages::messages::TransportServiceMessage;
use raiden_primitives::types::MessageIdentifier;
use serde::{
	Deserialize,
	Serialize,
};
use tokio::{
	select,
	sync::mpsc::{
		self,
		UnboundedReceiver,
		UnboundedSender,
	},
	time::interval,
};
use tokio_stream::wrappers::IntervalStream;
use tracing::error;

use crate::config::TransportConfig;

/// A generator for timeout which indicates if a message is ready for a retry.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct TimeoutGenerator {
	retries_count: u32,
	timeout: u8,
	timeout_max: u8,
	#[serde(skip_serializing, skip_deserializing)]
	next: Option<DateTime<Local>>,
	tries: u32,
}

impl TimeoutGenerator {
	/// Create a new instance of `TimeoutGenerator`.
	pub(crate) fn new(retries_count: u32, timeout: u8, timeout_max: u8) -> Self {
		Self { retries_count, timeout, timeout_max, next: None, tries: 1 }
	}

	/// Returns a boolean indicating whether a message is ready for a retry.
	pub(crate) fn ready(&mut self) -> bool {
		match self.next {
			Some(next) => {
				let now = Local::now();
				let reached_max_retries = self.tries >= self.retries_count;

				// Waited for `timeout` and did not reach `retries_count`.
				if next <= now && !reached_max_retries {
					self.next = Some(now + Duration::seconds(self.timeout as i64));
					self.tries += 1;
					return true
				}

				// At this point, we know that we have reached the `retries_count`,
				// so we start exponentially increasing the timeout.
				if next <= now && reached_max_retries {
					let timeout = min(self.timeout * 2, self.timeout_max);

					let set_timeout =
						if timeout < self.timeout_max { timeout } else { self.timeout_max };
					self.timeout = set_timeout;
					self.next = Some(now + Duration::seconds(self.timeout as i64));
					return true
				}

				false
			},
			None => {
				self.next = Some(Local::now() + Duration::seconds(self.timeout as i64));
				self.tries += 1;
				true
			},
		}
	}
}

/// A queue operation.
#[derive(Debug)]
pub(crate) enum QueueOp {
	Enqueue(MessageIdentifier),
	Dequeue(MessageIdentifier),
	Stop,
}

/// The data of the queued message.
#[derive(Serialize, Deserialize)]
struct QueuedMessageData {
	pub(self) message_identifier: MessageIdentifier,
	pub(self) timeout_generator: TimeoutGenerator,
}

/// A message queue which stores the message identifier and a timeout generator.
/// The timeout generator is used to check whether the message is ready for a retry or not.
/// If any messages in the queue is ready, a signal is sent back to the transport so that the
/// message can be sent over the wire.
pub(crate) struct RetryMessageQueue {
	transport_sender: UnboundedSender<TransportServiceMessage>,
	queue: Vec<QueuedMessageData>,
	channel_receiver: UnboundedReceiver<QueueOp>,
	retry_timeout: u8,
	retry_timeout_max: u8,
	retry_count: u32,
}

impl RetryMessageQueue {
	/// Create an instance of `RetryMessageQueue`.
	pub fn new(
		transport_sender: UnboundedSender<TransportServiceMessage>,
		transport_config: TransportConfig,
	) -> (Self, UnboundedSender<QueueOp>) {
		let (channel_sender, channel_receiver) = mpsc::unbounded_channel();
		(
			Self {
				channel_receiver,
				transport_sender,
				queue: vec![],
				retry_timeout: transport_config.retry_timeout,
				retry_timeout_max: transport_config.retry_timeout_max,
				retry_count: transport_config.retry_count,
			},
			channel_sender,
		)
	}

	/// Add message identifier to the queue.
	fn enqueue(&mut self, message_identifier: MessageIdentifier) {
		if self.queue.iter().any(|m| m.message_identifier == message_identifier) {
			return
		}
		self.queue.push(QueuedMessageData {
			message_identifier,
			timeout_generator: TimeoutGenerator::new(
				self.retry_count,
				self.retry_timeout,
				self.retry_timeout_max,
			),
		});
	}

	/// Remove the message identifier from queue.
	fn dequeue(&mut self, message_identifier: MessageIdentifier) {
		self.queue.retain(|data| data.message_identifier != message_identifier);
	}

	/// Loops forever and checks every certain interval for messages that are ready to be retried.
	pub async fn run(mut self) {
		let delay = IntervalStream::new(interval(StdDuration::from_millis(100)));
		tokio::pin!(delay);

		loop {
			select! {
				Some(queue_message) = self.channel_receiver.recv() => {
					match queue_message {
						QueueOp::Enqueue(message_identifier) => {
							self.enqueue(message_identifier);
						},
						QueueOp::Dequeue(message_identifier) => self.dequeue(message_identifier),
						QueueOp::Stop => {
							return;
						}
					}
				}
				_ = &mut delay.next() => {
					if self.queue.is_empty() {
						continue;
					}
					for message_data in self.queue.iter_mut().by_ref() {
						if message_data.timeout_generator.ready() {
							if let Err(e) = self.transport_sender.send(TransportServiceMessage::Send(message_data.message_identifier)) {
								error!(
									message = "Failed to send message to transport",
									message_identifier = message_data.message_identifier,
									error = format!("{:?}", e)
								);
							}
						}
					};
				}
			}
		}
	}
}
