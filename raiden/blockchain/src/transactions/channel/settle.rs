use std::sync::Arc;

use raiden_primitives::{
	hashing::hash_balance_data,
	types::{
		Address,
		BlockHash,
		BlockId,
		ChannelIdentifier,
		GasLimit,
		GasPrice,
		LockedAmount,
		Locksroot,
		TokenAmount,
		TransactionHash,
		U256,
	},
};
use raiden_state_machine::types::ChannelStatus;
use web3::{
	contract::Options,
	types::BlockNumber,
	Transport,
	Web3,
};

use crate::{
	contracts::GasMetadata,
	proxies::{
		Account,
		ChannelData,
		ParticipantsDetails,
		ProxyError,
		TokenNetworkProxy,
	},
	transactions::Transaction,
};

/// On-chain data to validate settling a channel.
#[derive(Clone)]
pub struct ChannelSettleTransactionData {
	channel_onchain_details: ChannelData,
	participants_details: ParticipantsDetails,
}

/// Parameters required for settling a channel.
#[derive(Clone)]
pub struct ChannelSettleTransactionParams {
	pub(crate) channel_identifier: ChannelIdentifier,
	pub(crate) our_transferred_amount: TokenAmount,
	pub(crate) our_locked_amount: LockedAmount,
	pub(crate) our_locksroot: Locksroot,
	pub(crate) partner_address: Address,
	pub(crate) partner_transferred_amount: TokenAmount,
	pub(crate) partner_locked_amount: LockedAmount,
	pub(crate) partner_locksroot: Locksroot,
}

/// Channel settle transaction type.
pub struct ChannelSettleTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) token_network: TokenNetworkProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelSettleTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = TransactionHash;
	type Params = ChannelSettleTransactionParams;
	type Data = ChannelSettleTransactionData;

	async fn onchain_data(
		&self,
		params: Self::Params,
		at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let channel_onchain_details = self
			.token_network
			.channel_details(
				Some(params.channel_identifier),
				self.account.address(),
				params.partner_address,
				at_block_hash,
			)
			.await?;

		let participants_details = self
			.token_network
			.participants_details(
				params.channel_identifier,
				self.account.address(),
				params.partner_address,
				Some(at_block_hash),
			)
			.await?;

		Ok(ChannelSettleTransactionData { channel_onchain_details, participants_details })
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		_block: BlockHash,
	) -> Result<(), ProxyError> {
		let current_block_number: U256 =
			self.web3.eth().block_number().await.map_err(ProxyError::Web3)?.as_u64().into();

		if current_block_number < data.channel_onchain_details.settle_block_number {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Settle cannot be called before the settlement period ends. \
                         This call should never have been attempted"
			)))
		}

		if data.channel_onchain_details.status != ChannelStatus::Closed {
			return Err(ProxyError::Recoverable(format!(
				"The channel was not closed at the provided block"
			)))
		}

		let our_balance_hash = hash_balance_data(
			params.our_transferred_amount,
			params.our_locked_amount,
			params.our_locksroot,
		)
		.map_err(|e| {
			ProxyError::BrokenPrecondition(format!("Could not hash balance data: {}", e))
		})?;
		let partner_balance_hash = hash_balance_data(
			params.partner_transferred_amount,
			params.partner_locked_amount,
			params.partner_locksroot,
		)
		.map_err(|e| {
			ProxyError::BrokenPrecondition(format!("Could not hash balance data: {}", e))
		})?;

		if data.participants_details.our_details.balance_hash != our_balance_hash {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Our balance hash does not match the on-chain value"
			)))
		}
		if data.participants_details.partner_details.balance_hash != partner_balance_hash {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Partner balance hash does not match the on-chain value"
			)))
		}

		Ok(())
	}

	async fn submit(
		&self,
		params: Self::Params,
		_data: Self::Data,
		gas_estimate: GasLimit,
		gas_price: GasPrice,
	) -> Result<Self::Output, ProxyError> {
		let nonce = self.account.peek_next_nonce().await;
		self.account.next_nonce().await;

		let receipt = self
			.token_network
			.contract
			.signed_call_with_confirmations(
				"settleChannel",
				(
					params.channel_identifier,
					self.account.address(),
					params.our_transferred_amount,
					params.our_locked_amount,
					params.our_locksroot,
					params.partner_address,
					params.partner_transferred_amount,
					params.partner_locked_amount,
					params.partner_locksroot,
				),
				Options::with(|opt| {
					opt.value = Some(GasLimit::from(0));
					opt.gas = Some(gas_estimate);
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
				1,
				self.account.private_key(),
			)
			.await?;

		Ok(receipt.transaction_hash)
	}

	async fn validate_postconditions(
		&self,
		params: Self::Params,
		_block: BlockHash,
	) -> Result<Self::Output, ProxyError> {
		let failed_at = self
			.web3
			.eth()
			.block(BlockId::Number(BlockNumber::Latest))
			.await
			.map_err(ProxyError::Web3)?
			.ok_or(ProxyError::Recoverable("Block not found".to_string()))?;

		let failed_at_blocknumber = failed_at.number.unwrap();
		let failed_at_blockhash = failed_at.hash.unwrap();

		self.account
			.check_for_insufficient_eth(
				self.gas_metadata.get("TokenNetwork.settle").into(),
				failed_at_blocknumber,
			)
			.await?;

		let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

		if data.channel_onchain_details.status == ChannelStatus::Settled ||
			data.channel_onchain_details.status == ChannelStatus::Removed
		{
			return Err(ProxyError::Recoverable(
				"Cannot call settle on a channel that has been settled already".to_owned(),
			))
		}

		if data.channel_onchain_details.status != ChannelStatus::Opened {
			return Err(ProxyError::Recoverable(format!(
				"The channel is still open. It cannot be settled"
			)))
		}

		let failed_at_blocknumber: U256 = failed_at_blocknumber.as_u64().into();
		let is_settle_window_over = data.channel_onchain_details.status == ChannelStatus::Closed &&
			failed_at_blocknumber > data.channel_onchain_details.settle_block_number;
		if !is_settle_window_over {
			return Err(ProxyError::Recoverable(format!(
				"The Channel cannot be settled before settlement window is over."
			)))
		}

		let our_balance_hash = hash_balance_data(
			params.our_transferred_amount,
			params.our_locked_amount,
			params.our_locksroot,
		)
		.map_err(|e| ProxyError::Recoverable(format!("Could not hash balance data: {}", e)))?;
		let partner_balance_hash = hash_balance_data(
			params.partner_transferred_amount,
			params.partner_locked_amount,
			params.partner_locksroot,
		)
		.map_err(|e| ProxyError::Recoverable(format!("Could not hash balance data: {}", e)))?;

		if data.participants_details.our_details.balance_hash != our_balance_hash {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Our balance hash does not match the on-chain value"
			)))
		}
		if data.participants_details.partner_details.balance_hash != partner_balance_hash {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Partner balance hash does not match the on-chain value"
			)))
		}

		Err(ProxyError::Recoverable(format!(
			"Settle channel failed. Gas estimation failed for
            unknown reason. Reference block {} - {}",
			failed_at_blockhash, failed_at_blocknumber,
		)))
	}

	async fn estimate_gas(
		&self,
		params: Self::Params,
		_data: Self::Data,
	) -> Result<(GasLimit, GasPrice), ProxyError> {
		let nonce = self.account.peek_next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(ProxyError::Web3)?;

		self.token_network
			.contract
			.estimate_gas(
				"settleChannel",
				(
					params.channel_identifier,
					self.account.address(),
					params.our_transferred_amount,
					params.our_locked_amount,
					params.our_locksroot,
					params.partner_address,
					params.partner_transferred_amount,
					params.partner_locked_amount,
					params.partner_locksroot,
				),
				self.account.address(),
				Options::with(|opt| {
					opt.value = Some(GasLimit::from(0));
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
			)
			.await
			.map(|estimate| (estimate, gas_price))
			.map_err(ProxyError::ChainError)
	}
}
