use matrix_sdk::HttpError;
use thiserror::Error;

use crate::primitives::QueueIdentifier;

use self::messages::Message;

pub mod matrix;
pub mod messages;

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
    async fn send(&self, queue_identifier: QueueIdentifier, message: Message) -> Result<(), TransportError>;
    async fn process(mut self);
}
