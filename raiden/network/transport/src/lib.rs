//! Implements Raiden protocol messages and matrix network integration to exchange messages between
//! nodes over the wire.
use matrix_sdk::HttpError;
use raiden_network_messages::messages::OutgoingMessage;
use raiden_primitives::types::QueueIdentifier;
use thiserror::Error;

pub mod config;
pub mod matrix;
#[cfg(test)]
mod tests;
pub mod types;

/// The transport error type.
#[derive(Error, Debug)]
pub enum TransportError {
	#[error("Could not initialize transport: `{0}`")]
	Init(String),
	#[error("Could to sync events: `{0}`")]
	Sync(String),
	#[error("Could to send messages: `{0}`")]
	Send(HttpError),
	#[error("Error: `{0}`")]
	Other(String),
}

#[async_trait::async_trait]
pub trait Transport {
	async fn init(&self) -> Result<(), TransportError>;
	async fn send(
		&self,
		queue_identifier: QueueIdentifier,
		message: OutgoingMessage,
	) -> Result<(), TransportError>;
	async fn process(mut self);
}
