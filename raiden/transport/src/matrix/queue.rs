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
use raiden_state_machine::types::{
	AddressMetadata,
	QueueIdentifier,
};
use serde_json::json;
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
use web3::types::Address;

use crate::{
	config::TransportConfig,
	messages::{
		Message,
		TransportServiceMessage,
	},
};

#[derive(Clone)]
struct TimeoutGenerator {
	retries_count: u32,
	timeout: u8,
	timeout_max: u8,
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

struct MessageData {
	pub(self) queue_identifier: QueueIdentifier,
	pub(self) message: Message,
	pub(self) text: String,
	pub(self) timeout_generator: TimeoutGenerator,
	pub(self) address_metadata: AddressMetadata,
}

type MessageInfo = (QueueIdentifier, Message);

pub struct RetryMessageQueue {
	transport_sender: UnboundedSender<TransportServiceMessage>,
	recipient: Address,
	queue: Vec<MessageData>,
	channel_receiver: UnboundedReceiver<MessageInfo>,
	retry_timeout: u8,
	retry_timeout_max: u8,
	retry_count: u32,
}

impl RetryMessageQueue {
	pub fn new(
		recipient: Address,
		transport_sender: UnboundedSender<TransportServiceMessage>,
		transport_config: TransportConfig,
	) -> (Self, UnboundedSender<MessageInfo>) {
		let (channel_sender, channel_receiver) = mpsc::unbounded_channel();
		(
			Self {
				recipient,
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

	pub fn enqueue(&mut self, queue_identifier: QueueIdentifier, message: Message) {
		if self
			.queue
			.iter()
			.any(|m| m.message.message_identifier == message.message_identifier)
		{
			return
		}

		let message_text = json!(message).to_string();
		let address_metadata = message.recipient_metadata.clone();
		self.queue.push(MessageData {
			queue_identifier,
			message,
			address_metadata,
			text: message_text,
			timeout_generator: TimeoutGenerator::new(
				self.retry_count,
				self.retry_timeout,
				self.retry_timeout_max,
			),
		});
	}

	pub async fn run(mut self) {
		let delay = IntervalStream::new(interval(StdDuration::from_millis(1000)));
		tokio::pin!(delay);

		loop {
			select! {
				Some((queue_identifier, message)) = self.channel_receiver.recv() => {
					self.enqueue(queue_identifier, message);
				}
				_ = &mut delay.next() => {
					if self.queue.is_empty() {
						continue;
					}
					for message_data in self.queue.iter_mut().by_ref() {
						if message_data.timeout_generator.ready() {
							let _ = self.transport_sender.send(TransportServiceMessage::Send(message_data.message.clone()));
						}
					};
				}
			}
		}
	}
}
