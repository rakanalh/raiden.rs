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

use crate::config::TransportConfig;

#[derive(Clone, Serialize, Deserialize)]
struct TimeoutGenerator {
	retries_count: u32,
	timeout: u8,
	timeout_max: u8,
	#[serde(skip_serializing, skip_deserializing)]
	next: Option<DateTime<Local>>,
	tries: u32,
}

impl TimeoutGenerator {
	fn new(retries_count: u32, timeout: u8, timeout_max: u8) -> Self {
		Self { retries_count, timeout, timeout_max, next: None, tries: 1 }
	}

	fn ready(&mut self) -> bool {
		match self.next {
			Some(next) => {
				let now = Local::now();
				let reached_max_retries = self.tries >= self.retries_count;

				// Waited for `timeout` and reached `retries_count`.
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
				false
			},
		}
	}
}

pub(crate) enum QueueOp {
	Enqueue(MessageIdentifier),
	Dequeue(MessageIdentifier),
}

#[derive(Serialize, Deserialize)]
struct QueuedMessageData {
	pub(self) message_identifier: MessageIdentifier,
	pub(self) timeout_generator: TimeoutGenerator,
}

pub(crate) struct RetryMessageQueue {
	transport_sender: UnboundedSender<TransportServiceMessage>,
	queue: Vec<QueuedMessageData>,
	channel_receiver: UnboundedReceiver<QueueOp>,
	retry_timeout: u8,
	retry_timeout_max: u8,
	retry_count: u32,
}

impl RetryMessageQueue {
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

	fn process_queue_message(&mut self, queue_message: QueueOp) {
		match queue_message {
			QueueOp::Enqueue(message_identifier) => {
				self.enqueue(message_identifier);
			},
			QueueOp::Dequeue(message_identifier) => self.dequeue(message_identifier),
		}
	}

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

	fn dequeue(&mut self, message_identifier: MessageIdentifier) {
		self.queue.retain(|data| data.message_identifier != message_identifier);
	}

	pub async fn run(mut self) {
		let delay = IntervalStream::new(interval(StdDuration::from_millis(1000)));
		tokio::pin!(delay);

		loop {
			select! {
				Some(queue_message) = self.channel_receiver.recv() => {
					self.process_queue_message(queue_message);
					if self.queue.is_empty() {
						return;
					}
				}
				_ = &mut delay.next() => {
					if self.queue.is_empty() {
						return;
					}
					for message_data in self.queue.iter_mut().by_ref() {
						if message_data.timeout_generator.ready() {
							let _ = self.transport_sender.send(TransportServiceMessage::Send(message_data.message_identifier));
						}
					};
				}
			}
		}
	}
}
