use std::sync::Arc;

use raiden_primitives::{
	constants::LOCKSROOT_OF_NO_LOCKS,
	types::{
		Address,
		BlockHash,
		BlockId,
		ChannelIdentifier,
		GasLimit,
		GasPrice,
		TokenAmount,
		TransactionHash,
	},
};
use raiden_state_machine::{
	machine::channel::utils::compute_locksroot,
	types::{
		ChannelStatus,
		PendingLocksState,
	},
};
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
		ParticipantDetails,
		ProxyError,
		TokenNetworkProxy,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct ChannelUnlockTransactionData {
	channel_onchain_details: ChannelData,
	sender_details: ParticipantDetails,
}

#[derive(Clone)]
pub struct ChannelUnlockTransactionParams {
	pub(crate) channel_identifier: ChannelIdentifier,
	pub(crate) sender: Address,
	pub(crate) receiver: Address,
	pub(crate) pending_locks: PendingLocksState,
}

pub struct ChannelUnlockTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) token_network: TokenNetworkProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelUnlockTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = TransactionHash;
	type Params = ChannelUnlockTransactionParams;
	type Data = ChannelUnlockTransactionData;

	async fn onchain_data(
		&self,
		params: Self::Params,
		at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let channel_onchain_details = self
			.token_network
			.channel_details(
				Some(params.channel_identifier),
				params.sender,
				params.receiver,
				at_block_hash,
			)
			.await?;

		let sender_details = self
			.token_network
			.participant_details(
				params.channel_identifier,
				params.sender,
				params.receiver,
				Some(at_block_hash),
			)
			.await?;

		Ok(ChannelUnlockTransactionData { channel_onchain_details, sender_details })
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		_block: BlockHash,
	) -> Result<(), ProxyError> {
		if data.channel_onchain_details.status != ChannelStatus::Settled {
			return Err(ProxyError::BrokenPrecondition(
				"The channel was not settled at the provided block".to_owned(),
			))
		}

		let local_locksroot = compute_locksroot(&params.pending_locks);
		if data.sender_details.locksroot != local_locksroot {
			return Err(ProxyError::BrokenPrecondition(
				"The provided locksroot does not correspond to the on-chain locksroot".to_owned(),
			))
		}
		if data.sender_details.locked_amount == TokenAmount::zero() {
			return Err(ProxyError::BrokenPrecondition(
				"The provided locked amount on-chain is 0".to_owned(),
			))
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
		let nonce = self.account.next_nonce().await;

		let leaves_packed = params.pending_locks.locks.iter().fold(vec![], |mut current, lock| {
			current.extend_from_slice(&lock.0);
			current
		});
		let receipt = self
			.token_network
			.contract
			.signed_call_with_confirmations(
				"unlock",
				(
					params.channel_identifier,
					self.account.address(),
					params.sender,
					params.receiver,
					leaves_packed,
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
				self.gas_metadata.get("TokenNetwork.unlock").into(),
				failed_at_blocknumber,
			)
			.await?;

		let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

		let channel_status = data.channel_onchain_details.status;
		if channel_status == ChannelStatus::Opened ||
			channel_status == ChannelStatus::Closed ||
			channel_status == ChannelStatus::Closing
		{
			return Err(ProxyError::Recoverable(
				"Cannot call close on a channel that has not been settled already".to_owned(),
			))
		}

		if data.sender_details.locksroot == *LOCKSROOT_OF_NO_LOCKS {
			return Err(ProxyError::Recoverable("The locks are already unlocked".to_owned()))
		}

		Err(ProxyError::Recoverable(format!(
			"Unlock channel failed. Gas estimation failed for
            unknown reason. Reference block {} - {}",
			failed_at_blockhash, failed_at_blocknumber,
		)))
	}

	async fn estimate_gas(
		&self,
		params: Self::Params,
		_data: Self::Data,
	) -> Result<(GasLimit, GasPrice), ()> {
		let nonce = self.account.peek_next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(|_| ())?;

		let leaves_packed = params.pending_locks.locks.iter().fold(vec![], |mut current, lock| {
			current.extend_from_slice(&lock.0);
			current
		});
		self.token_network
			.contract
			.estimate_gas(
				"unlock",
				(
					params.channel_identifier,
					self.account.address(),
					params.sender,
					params.receiver,
					leaves_packed,
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
			.map_err(|_| ())
	}
}
