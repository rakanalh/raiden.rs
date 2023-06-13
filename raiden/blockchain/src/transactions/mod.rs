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

/// A trait to be implemented by on-chain transactions.
#[async_trait::async_trait]
pub trait Transaction {
	/// The type which is returned as a result of a successful transaction execution.
	type Output: Send + Sync;
	/// The params type to be passed down to transaction.
	type Params: Clone + Send + Sync;
	/// The on-chain data placeholder.
	type Data: Clone + Send + Sync;

	/// Fetch data residing on-chain for validation prior / post execution.
	async fn onchain_data(
		&self,
		params: Self::Params,
		at_block_hash: H256,
	) -> Result<Self::Data, ProxyError>;

	/// Validate pre-conditions needed to execute the transaction.
	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		at_block_hash: H256,
	) -> Result<(), ProxyError>;

	/// Submit transaction on-chain.
	async fn submit(
		&self,
		params: Self::Params,
		data: Self::Data,
		gas_estimate: U256,
		gas_price: U256,
	) -> Result<Self::Output, ProxyError>;

	/// Validate conditions after the execution of the transaction in case the execution failed.
	async fn validate_postconditions(
		&self,
		params: Self::Params,
		at_block_hash: H256,
	) -> Result<Self::Output, ProxyError>;

	/// Estimate gas for the transaction.
	async fn estimate_gas(
		&self,
		params: Self::Params,
		data: Self::Data,
	) -> Result<(U256, U256), ProxyError>;

	/// Execute transactions that are required prior to executing the current one.
	///
	/// Some transactions like deposit might need an approve call before.
	async fn execute_prerequisite(
		&self,
		_params: Self::Params,
		_data: Self::Data,
	) -> Result<(), ProxyError> {
		Ok(())
	}

	/// Validate preconditions, execute the transaction and if failed, validate post conditions.
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

	/// Acquire lock, if needed.
	async fn acquire_lock(&self) -> Option<RwLockWriteGuard<bool>> {
		None
	}
}
