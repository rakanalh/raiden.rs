mod channel;
mod secret;
mod token;
mod user_deposit;

use raiden_primitives::types::{
	H256,
	U256,
};
use tokio::sync::RwLockWriteGuard;

use crate::proxies::ProxyError;
pub use crate::transactions::{
	channel::*,
	secret::*,
	token::*,
	user_deposit::*,
};

#[async_trait::async_trait]
pub trait Transaction {
	type Output: Send + Sync;
	type Params: Clone + Send + Sync;
	type Data: Clone + Send + Sync;

	async fn onchain_data(
		&self,
		params: Self::Params,
		at_block_hash: H256,
	) -> Result<Self::Data, ProxyError>;

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		at_block_hash: H256,
	) -> Result<(), ProxyError>;

	async fn submit(
		&self,
		params: Self::Params,
		data: Self::Data,
		gas_estimate: U256,
		gas_price: U256,
	) -> Result<Self::Output, ProxyError>;

	async fn validate_postconditions(
		&self,
		params: Self::Params,
		at_block_hash: H256,
	) -> Result<Self::Output, ProxyError>;

	async fn estimate_gas(
		&self,
		params: Self::Params,
		data: Self::Data,
	) -> Result<(U256, U256), ProxyError>;

	async fn execute_prerequisite(
		&self,
		_params: Self::Params,
		_data: Self::Data,
	) -> Result<(), ProxyError> {
		Ok(())
	}

	async fn execute(
		&self,
		params: Self::Params,
		at_block_hash: H256,
	) -> Result<Self::Output, ProxyError> {
		let data = self.onchain_data(params.clone(), at_block_hash).await?;
		self.validate_preconditions(params.clone(), data.clone(), at_block_hash).await?;

		let _lock_guard = self.acquire_lock().await;

		self.execute_prerequisite(params.clone(), data.clone()).await?;
		let (gas_estimate, gas_price) = self.estimate_gas(params.clone(), data.clone()).await?;
		match self.submit(params.clone(), data, gas_estimate, gas_price).await {
			Ok(result) => Ok(result),
			Err(_) => return self.validate_postconditions(params, at_block_hash).await,
		}
	}

	async fn acquire_lock(&self) -> Option<RwLockWriteGuard<bool>> {
		None
	}
}
