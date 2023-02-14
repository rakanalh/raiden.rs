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
use matrix_sdk::config::SyncSettings;
use raiden_state_machine::types::QueueIdentifier;
use tokio::{
	select,
	sync::mpsc::{
		self,
		UnboundedSender,
	},
};
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{
	queue::RetryMessageQueue,
	MatrixClient,
};
use crate::{
	config::TransportConfig,
	messages::{
		Message,
		TransportServiceMessage,
	},
};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + Sync + 'a>>;

pub struct MatrixService {
	config: TransportConfig,
	client: Arc<MatrixClient>,
	sender: UnboundedSender<TransportServiceMessage>,
	receiver: UnboundedReceiverStream<TransportServiceMessage>,
	message_queues: HashMap<QueueIdentifier, UnboundedSender<(QueueIdentifier, Message)>>,
	running_futures: FuturesUnordered<BoxFuture<'static, ()>>,
}

impl MatrixService {
	pub fn new(
		config: TransportConfig,
		client: Arc<MatrixClient>,
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
		let sync_settings = SyncSettings::new().timeout(Duration::from_secs(30));
		loop {
			select! {
				_response = self.client.sync_once(sync_settings.clone()).fuse() => {
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
}
