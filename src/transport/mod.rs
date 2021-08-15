pub mod matrix;

#[async_trait::async_trait]
pub trait Transport {
    async fn init(&self);
    async fn sync(&self);
}
