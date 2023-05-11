use std::{
	thread,
	time::Duration,
};

use futures_util::FutureExt;
use raiden_network_messages::messages::TransportServiceMessage;
use tokio::sync::mpsc;

use crate::{
	config::{
		MatrixTransportConfig,
		TransportConfig,
	},
	matrix::queue::{
		QueueOp,
		RetryMessageQueue,
		TimeoutGenerator,
	},
};

#[test]
fn test_timeout_generator() {
	// 5 second timeout, 60 secs limit, 10 retries
	let mut timeout_generator = TimeoutGenerator::new(10, 5, 60);
	// ready
	assert!(timeout_generator.ready());
	// Not ready after 1 second
	thread::sleep(Duration::from_secs(1));
	assert!(!timeout_generator.ready());
	// Not ready after 3 seconds
	thread::sleep(Duration::from_secs(3));
	assert!(!timeout_generator.ready());
	// Ready after 1 second, 5 in total
	thread::sleep(Duration::from_secs(1));
	assert!(timeout_generator.ready());

	let mut timeout_generator = TimeoutGenerator::new(10, 1, 60);
	fn reach_max_retries(timeout_generator: &mut TimeoutGenerator) {
		// Reach max retries
		for _ in 1..11 {
			thread::sleep(Duration::from_secs(1));
			assert!(timeout_generator.ready());
		}
	}

	reach_max_retries(&mut timeout_generator);

	// When max retries is reached, we start to increase timeout exponentially.
	// Next step needs 2 seconds to succeed
	thread::sleep(Duration::from_secs(1));
	assert!(!timeout_generator.ready());
	thread::sleep(Duration::from_secs(1));
	assert!(timeout_generator.ready());
	// Next step needs 4 seconds to succeed
	thread::sleep(Duration::from_secs(2));
	assert!(!timeout_generator.ready());
	thread::sleep(Duration::from_secs(2));
	assert!(timeout_generator.ready());
}

#[tokio::test]
async fn test_retry_message_queue() {
	let (transport_sender, mut transport_receiver) = mpsc::unbounded_channel();
	let (message_queue, queue_sender) = RetryMessageQueue::new(
		transport_sender,
		TransportConfig {
			retry_timeout: 5,
			retry_timeout_max: 60,
			retry_count: 10,
			matrix: MatrixTransportConfig { homeserver_url: "http://test.com".to_owned() },
		},
	);
	let (job, _handle) = FutureExt::remote_handle(message_queue.run());
	tokio::spawn(job);

	let message_identifier = 1;
	let _ = queue_sender.send(QueueOp::Enqueue(message_identifier));

	let received_identifier = transport_receiver.recv().await;
	assert!(received_identifier.is_some());
	assert_eq!(received_identifier.unwrap(), TransportServiceMessage::Send(message_identifier));

	let _ = queue_sender.send(QueueOp::Dequeue(message_identifier));

	let received_identifier = transport_receiver.try_recv();
	assert!(received_identifier.is_err());
}
