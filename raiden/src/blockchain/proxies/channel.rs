use web3::{
	types::{Address, H256, U256},
	Transport,
};

use super::{
	common::{Account, Result},
	TokenNetworkProxy,
};

#[derive(Clone)]
pub struct ChannelProxy<T: Transport> {
	pub token_network: TokenNetworkProxy<T>,
}

impl<T> ChannelProxy<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	pub fn new(token_network: TokenNetworkProxy<T>) -> Self {
		Self { token_network }
	}

	pub async fn approve_and_set_total_deposit(
		&self,
		account: Account<T>,
		channel_identifier: U256,
		partner: Address,
		total_deposit: U256,
		block_hash: H256,
	) -> Result<()> {
		self.token_network
			.approve_and_set_total_deposit(
				account,
				channel_identifier,
				partner,
				total_deposit,
				block_hash,
			)
			.await
	}
}
