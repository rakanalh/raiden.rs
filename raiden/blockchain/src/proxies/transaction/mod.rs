mod channel;

pub use channel::*;
use web3::types::{
	H256,
	U256,
};

use super::ProxyError;

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
	) -> Result<(U256, U256), ()>;

	async fn execute(
		&self,
		params: Self::Params,
		at_block_hash: H256,
	) -> Result<Self::Output, ProxyError> {
		let data = self.onchain_data(params.clone(), at_block_hash).await?;

		if let Ok(_) = self.validate_postconditions(params.clone(), at_block_hash).await {
			if let Ok((gas_estimate, gas_price)) =
				self.estimate_gas(params.clone(), data.clone()).await
			{
				return self.submit(params, data, gas_estimate, gas_price).await
			}
		}

		self.validate_postconditions(params, at_block_hash).await
	}
}
