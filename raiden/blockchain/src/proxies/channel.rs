use ethabi::ethereum_types::H256;
use raiden_primitives::types::{
	Address,
	BalanceHash,
	BlockHash,
	ChannelIdentifier,
	Nonce,
	Signature,
	TokenAmount,
	TransactionHash,
};
use web3::Transport;

use super::{
	common::{
		Account,
		Result,
	},
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
		channel_identifier: ChannelIdentifier,
		partner: Address,
		total_deposit: TokenAmount,
		block_hash: BlockHash,
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

	pub async fn close(
		&self,
		account: Account<T>,
		partner: Address,
		channel_identifier: ChannelIdentifier,
		nonce: Nonce,
		balance_hash: BalanceHash,
		additional_hash: H256,
		non_closing_signature: Signature,
		closing_signature: Signature,
		block_hash: BlockHash,
	) -> Result<TransactionHash> {
		self.token_network
			.close(
				account,
				partner,
				channel_identifier,
				nonce,
				balance_hash,
				additional_hash,
				non_closing_signature,
				closing_signature,
				block_hash,
			)
			.await
	}
}
