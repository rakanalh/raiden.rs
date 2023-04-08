use ethabi::ethereum_types::H256;
use raiden_primitives::types::{
	Address,
	BalanceHash,
	BlockExpiration,
	BlockHash,
	ChannelIdentifier,
	Nonce,
	Signature,
	TokenAmount,
	TransactionHash,
};
use raiden_state_machine::types::PendingLocksState;
use web3::Transport;

use super::{
	common::{
		Account,
		Result,
	},
	TokenNetworkProxy,
};
use crate::transactions::WithdrawInput;

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

	pub async fn set_total_withdraw(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		total_withdraw: TokenAmount,
		participant: Address,
		partner: Address,
		participant_signature: Signature,
		partner_signature: Signature,
		expiration_block: BlockExpiration,
		block_hash: BlockHash,
	) -> Result<()> {
		self.token_network
			.set_total_withdraw(
				account,
				channel_identifier,
				total_withdraw,
				participant,
				partner,
				participant_signature,
				partner_signature,
				expiration_block,
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

	pub async fn update_transfer(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		nonce: Nonce,
		partner: Address,
		balance_hash: BalanceHash,
		additional_hash: H256,
		closing_signature: Signature,
		non_closing_signature: Signature,
		block_hash: BlockHash,
	) -> Result<TransactionHash> {
		self.token_network
			.update_transfer(
				account,
				channel_identifier,
				nonce,
				partner,
				balance_hash,
				additional_hash,
				closing_signature,
				non_closing_signature,
				block_hash,
			)
			.await
	}

	pub async fn unlock(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		sender: Address,
		receiver: Address,
		pending_locks: PendingLocksState,
		block_hash: BlockHash,
	) -> Result<TransactionHash> {
		self.token_network
			.unlock(account, channel_identifier, sender, receiver, pending_locks, block_hash)
			.await
	}

	pub async fn coop_settle(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		withdraw_partner: WithdrawInput,
		withdraw_initiator: WithdrawInput,
		block_hash: BlockHash,
	) -> Result<TransactionHash> {
		self.token_network
			.coop_settle(
				account,
				channel_identifier,
				withdraw_partner,
				withdraw_initiator,
				block_hash,
			)
			.await
	}
}
