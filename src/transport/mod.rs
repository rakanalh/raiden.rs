use thiserror::Error;

pub mod matrix;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Could not initialize transport: `{0}`")]
    Init(String),
    #[error("Could to sync events: `{0}`")]
    Sync(String)
}

#[async_trait::async_trait]
pub trait Transport {
    async fn init(&self) -> Result<(), TransportError>;
    async fn sync(&self);
}
