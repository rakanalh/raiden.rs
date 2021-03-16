use std::sync::Arc;

use tokio::sync::Mutex;
use web3::types::U256;

#[derive(Clone)]
pub struct Nonce {
    inner: Arc<Mutex<U256>>,
}

impl Nonce {
    pub fn new(current: U256) -> Self {
        Self {
            inner: Arc::new(Mutex::new(current)),
        }
    }

    pub async fn next(&self) -> U256 {
		let mut inner = self.inner.lock().await;
        *inner += U256::from(1);
        inner.clone()
    }
}
