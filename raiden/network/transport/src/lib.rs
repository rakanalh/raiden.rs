use matrix_sdk::HttpError;
use raiden_network_messages::messages::Message;
use raiden_state_machine::types::QueueIdentifier;
use thiserror::Error;

pub mod config;
pub mod matrix;
pub mod types;

#[derive(Error, Debug)]
pub enum TransportError {
	#[error("Could not initialize transport: `{0}`")]
	Init(String),
	#[error("Could to sync events: `{0}`")]
	Sync(String),
	#[error("Could to send messages: `{0}`")]
	Send(HttpError),
}

#[async_trait::async_trait]
pub trait Transport {
	async fn init(&self) -> Result<(), TransportError>;
	async fn send(
		&self,
		queue_identifier: QueueIdentifier,
		message: Message,
	) -> Result<(), TransportError>;
	async fn process(mut self);
}
