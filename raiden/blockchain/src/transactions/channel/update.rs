use std::sync::Arc;

use raiden_primitives::{
	packing::{
		pack_balance_proof,
		pack_balance_proof_message,
	},
	signing::recover,
	types::{
		Address,
		BalanceHash,
		BlockHash,
		BlockId,
		CanonicalIdentifier,
		ChainID,
		ChannelIdentifier,
		GasLimit,
		GasPrice,
		MessageTypeId,
		Nonce,
		Signature,
		TransactionHash,
		H256,
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
		ParticipantDetails,
		ProxyError,
		TokenNetworkProxy,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct ChannelUpdateTransferTransactionData {
	chain_id: ChainID,
	channel_onchain_details: ChannelData,
	closer_details: ParticipantDetails,
}

#[derive(Clone)]
pub struct ChannelUpdateTransferTransactionParams {
	pub(crate) channel_identifier: ChannelIdentifier,
	pub(crate) nonce: Nonce,
	pub(crate) partner: Address,
	pub(crate) balance_hash: BalanceHash,
	pub(crate) additional_hash: H256,
	pub(crate) closing_signature: Signature,
	pub(crate) non_closing_signature: Signature,
}

pub struct ChannelUpdateTransferTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) token_network: TokenNetworkProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelUpdateTransferTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = TransactionHash;
	type Params = ChannelUpdateTransferTransactionParams;
	type Data = ChannelUpdateTransferTransactionData;

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
				params.partner,
				at_block_hash,
			)
			.await?;

		let closer_details = self
			.token_network
			.participant_details(
				params.channel_identifier,
				params.partner,
				self.account.address(),
				Some(at_block_hash),
			)
			.await?;

		let chain_id = self.token_network.chain_id(at_block_hash).await?;

		Ok(ChannelUpdateTransferTransactionData {
			channel_onchain_details,
			chain_id,
			closer_details,
		})
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		_block: BlockHash,
	) -> Result<(), ProxyError> {
		let canonical_identifier = CanonicalIdentifier {
			chain_identifier: data.chain_id,
			token_network_address: self.token_network.contract.address(),
			channel_identifier: params.channel_identifier,
		};

		let partner_signed_data = pack_balance_proof(
			params.nonce,
			params.balance_hash,
			params.additional_hash,
			canonical_identifier.clone(),
			MessageTypeId::BalanceProof,
		);

		let our_signed_data = pack_balance_proof_message(
			params.nonce,
			params.balance_hash,
			params.additional_hash,
			canonical_identifier.clone(),
			MessageTypeId::BalanceProof,
			params.closing_signature.clone(),
		);

		let partner_recovered_address =
			recover(&partner_signed_data.0, &params.closing_signature.0).map_err(|_| {
				ProxyError::Unrecoverable("Could not verify the closing signature".to_owned())
			})?;

		let our_recovered_address = recover(&our_signed_data.0, &params.non_closing_signature.0)
			.map_err(|_| {
				ProxyError::Unrecoverable("Could not verify the non-closing signature".to_owned())
			})?;

		if partner_recovered_address != params.partner {
			return Err(ProxyError::Unrecoverable("Invalid closing signature".to_owned()))
		}
		if our_recovered_address != self.account.address() {
			return Err(ProxyError::Unrecoverable("Invalid non-closing signature".to_owned()))
		}

		if data.channel_onchain_details.status != ChannelStatus::Closed {
			return Err(ProxyError::Recoverable(format!(
				"The channel was not closed at the provided block"
			)))
		}

		let current_block_number: U256 =
			self.web3.eth().block_number().await.map_err(ProxyError::Web3)?.as_u64().into();

		if data.channel_onchain_details.settle_block_number > current_block_number {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Update transfer cannot be called after settlement period. \
                 This call should never have been attempted"
			)))
		}

		if data.closer_details.nonce == params.nonce {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Update transfer was already done. \
                 This call should never have been attempted"
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
		let nonce = self.account.next_nonce().await;

		let receipt = self
			.token_network
			.contract
			.signed_call_with_confirmations(
				"updateNonClosingBalanceProof",
				(
					params.channel_identifier,
					params.partner,
					self.account.address(),
					params.balance_hash,
					params.nonce,
					params.additional_hash,
					params.closing_signature,
					params.non_closing_signature,
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
				self.gas_metadata.get("TokenNetwork.openChannel").into(),
				failed_at_blocknumber,
			)
			.await?;

		let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

		if data.channel_onchain_details.channel_identifier == ChannelIdentifier::zero() ||
			data.channel_onchain_details.channel_identifier > params.channel_identifier
		{
			return Err(ProxyError::Recoverable(
				"The provided channel identifier does not match the value on-chain \
                 at the block the update transfer was mined."
					.to_owned(),
			))
		}
		if data.channel_onchain_details.status == ChannelStatus::Settled ||
			data.channel_onchain_details.status == ChannelStatus::Removed
		{
			return Err(ProxyError::Recoverable(
				"Cannot call settle on a channel that has been settled already".to_owned(),
			))
		}

		if data.channel_onchain_details.settle_block_number < failed_at_blocknumber.as_u64().into()
		{
			return Err(ProxyError::Recoverable(
				"Update transfer transaction sent after settlement window".to_owned(),
			))
		}

		if data.closer_details.nonce != params.nonce {
			return Err(ProxyError::Recoverable(
				"Update transfer failed. The on-chain nonce is higher than our expected."
					.to_owned(),
			))
		}

		if data.channel_onchain_details.status == ChannelStatus::Closed ||
			data.channel_onchain_details.status == ChannelStatus::Closing ||
			data.channel_onchain_details.status == ChannelStatus::Opened
		{
			return Err(ProxyError::Recoverable("The channel state changed unexpectedly".to_owned()))
		}

		Err(ProxyError::Recoverable(format!(
			"UpdateTransfer channel failed. Gas estimation failed for
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

		self.token_network
			.contract
			.estimate_gas(
				"updateNonClosingBalanceProof",
				(
					params.channel_identifier,
					params.partner,
					self.account.address(),
					params.balance_hash,
					params.nonce,
					params.additional_hash,
					params.closing_signature,
					params.non_closing_signature,
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
