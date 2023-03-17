use std::sync::Arc;

use raiden_primitives::{
	constants::EMPTY_SIGNATURE,
	packing::pack_withdraw,
	signing::recover,
	types::{
		Address,
		BlockExpiration,
		BlockHash,
		BlockId,
		CanonicalIdentifier,
		ChainID,
		ChannelIdentifier,
		GasLimit,
		GasPrice,
		Signature,
		TokenAmount,
		U64,
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
pub struct ChannelSetTotalWithdrawTransactionData {
	pub(crate) chain_id: ChainID,
	pub(crate) channel_onchain_details: ChannelData,
	pub(crate) our_details: ParticipantDetails,
	pub(crate) partner_details: ParticipantDetails,
}

#[derive(Clone)]
pub struct ChannelSetTotalWithdrawTransactionParams {
	pub(crate) channel_identifier: ChannelIdentifier,
	pub(crate) participant: Address,
	pub(crate) participant2: Address,
	pub(crate) participant_signature: Signature,
	pub(crate) participant2_signature: Signature,
	pub(crate) total_withdraw: TokenAmount,
	pub(crate) expiration_block: BlockExpiration,
}

pub struct ChannelSetTotalWithdrawTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) token_network: TokenNetworkProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelSetTotalWithdrawTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = ();
	type Params = ChannelSetTotalWithdrawTransactionParams;
	type Data = ChannelSetTotalWithdrawTransactionData;

	async fn onchain_data(
		&self,
		params: Self::Params,
		at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let chain_id = self.token_network.chain_id(at_block_hash).await?;

		let channel_identifier = self
			.token_network
			.get_channel_identifier(params.participant, params.participant2, at_block_hash)
			.await?
			.ok_or(ProxyError::BrokenPrecondition("Block not found".to_string()))?;

		let channel_onchain_details = self
			.token_network
			.channel_details(
				Some(channel_identifier),
				self.account.address(),
				params.participant2,
				at_block_hash,
			)
			.await?;

		let our_details = match self
			.token_network
			.participant_details(
				channel_identifier,
				params.participant,
				params.participant2,
				Some(at_block_hash),
			)
			.await
		{
			Ok(our_details) => our_details,
			Err(_) =>
				self.token_network
					.participant_details(
						channel_identifier,
						params.participant,
						params.participant2,
						None,
					)
					.await?,
		};

		let partner_details = self
			.token_network
			.participant_details(
				channel_identifier,
				params.participant2,
				params.participant,
				Some(at_block_hash),
			)
			.await?;

		Ok(ChannelSetTotalWithdrawTransactionData {
			chain_id,
			channel_onchain_details,
			our_details,
			partner_details,
		})
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		at_block_hash: BlockHash,
	) -> Result<(), ProxyError> {
		if data.channel_onchain_details.status != ChannelStatus::Opened {
			return Err(ProxyError::BrokenPrecondition(format!(
				"The channel was not opened at the provided block \
                ({}). This call should never have been attempted.",
				at_block_hash
			)))
		}

		if params.total_withdraw <= data.our_details.withdrawn {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Current total withdraw ({}) is already larger \
                than the requested total withdraw amount ({})",
				data.our_details.withdrawn, params.total_withdraw,
			)))
		}

		let (total_channel_withdraw, total_channel_withdraw_overflow) =
			params.total_withdraw.overflowing_add(data.partner_details.withdrawn);
		let (total_channel_deposit, total_channel_deposit_overflow) =
			data.our_details.deposit.overflowing_add(data.partner_details.deposit);

		if total_channel_withdraw_overflow {
			return Err(ProxyError::BrokenPrecondition(format!("Withdraw overflow")))
		}
		if total_channel_deposit_overflow {
			return Err(ProxyError::BrokenPrecondition(format!("Deposit overflow")))
		}

		if total_channel_withdraw > total_channel_deposit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Total channel withdraw of {} is larger than the \
                 total channel deposit {}",
				total_channel_withdraw, total_channel_deposit,
			)))
		}

		let current_block_number: U64 =
			self.web3.eth().block_number().await.map_err(ProxyError::Web3)?.into();

		if params.expiration_block <= current_block_number {
			return Err(ProxyError::BrokenPrecondition(format!(
				"The current block number {} is already at expiration block {} or later",
				current_block_number, params.expiration_block
			)))
		}

		if params.participant_signature == *EMPTY_SIGNATURE {
			return Err(ProxyError::BrokenPrecondition(format!(
				"set_total_withdraw requires a valid participant signature",
			)))
		}

		if params.participant2_signature == *EMPTY_SIGNATURE {
			return Err(ProxyError::BrokenPrecondition(format!(
				"set_total_withdraw requires a valid partner signature",
			)))
		}

		let canonical_identifier = CanonicalIdentifier {
			chain_identifier: data.chain_id,
			token_network_address: self.token_network.contract.address(),
			channel_identifier: params.channel_identifier,
		};

		let participant_signed_data = pack_withdraw(
			canonical_identifier.clone(),
			params.participant,
			params.total_withdraw,
			params.expiration_block,
		);

		let participant_recovered_address =
			recover(&participant_signed_data.0, &params.participant_signature.0).map_err(|_| {
				ProxyError::Recoverable("Couldn't verify initiator withdraw signature".to_owned())
			})?;

		if participant_recovered_address != params.participant {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Invalid withdraw initiator signature",
			)))
		}

		let partner_signed_data = pack_withdraw(
			canonical_identifier,
			params.participant,
			params.total_withdraw,
			params.expiration_block,
		);

		let partner_recovered_address =
			recover(&partner_signed_data.0, &params.participant2_signature.0).map_err(|_| {
				ProxyError::Recoverable("Couldn't verify initiator withdraw signature".to_owned())
			})?;

		if partner_recovered_address != params.participant2 {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Invalid withdraw partner signature",
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

		self.token_network
			.contract
			.signed_call_with_confirmations(
				"setTotalWithdraw",
				(
					params.channel_identifier,
					params.participant,
					params.total_withdraw,
					params.expiration_block.0,
					params.participant_signature.0,
					params.participant2_signature.0,
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
		Ok(())
	}

	async fn validate_postconditions(
		&self,
		params: Self::Params,
		_at_block_hash: BlockHash,
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

		if data.channel_onchain_details.status == ChannelStatus::Closed {
			return Err(ProxyError::Recoverable(format!(
				"Withdraw failed because the channel was closed meanwhile",
			)))
		}

		if data.channel_onchain_details.status == ChannelStatus::Settled {
			return Err(ProxyError::Recoverable(format!(
				"Withdraw failed because the channel was settled meanwhile",
			)))
		}

		if data.channel_onchain_details.status == ChannelStatus::Removed {
			return Err(ProxyError::Recoverable(format!(
				"Withdraw failed because the channel was settled and unlocked meanwhile",
			)))
		}

		if params.total_withdraw <= data.our_details.withdrawn {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Current total withdraw ({}) is already larger \
                than the requested total withdraw amount ({})",
				data.our_details.withdrawn, params.total_withdraw,
			)))
		}

		let (total_channel_withdraw, total_channel_withdraw_overflow) =
			params.total_withdraw.overflowing_add(data.partner_details.withdrawn);
		let (total_channel_deposit, total_channel_deposit_overflow) =
			data.our_details.deposit.overflowing_add(data.partner_details.deposit);

		if total_channel_withdraw_overflow {
			return Err(ProxyError::BrokenPrecondition(format!("Withdraw overflow")))
		}
		if total_channel_deposit_overflow {
			return Err(ProxyError::BrokenPrecondition(format!("Deposit overflow")))
		}

		if total_channel_withdraw > total_channel_deposit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Total channel withdraw of {} is larger than the \
                 total channel deposit {}",
				total_channel_withdraw, total_channel_deposit,
			)))
		}

		let current_block_number: U64 =
			self.web3.eth().block_number().await.map_err(ProxyError::Web3)?.into();

		if params.expiration_block <= current_block_number {
			return Err(ProxyError::BrokenPrecondition(format!(
				"The current block number {} is already at expiration block {} or later",
				current_block_number, params.expiration_block
			)))
		}

		return Err(ProxyError::Recoverable(format!("withdraw failed for an unknown reason")))
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
				"setTotalWithdraw",
				(
					params.channel_identifier,
					params.participant,
					params.total_withdraw,
					params.expiration_block.0,
					params.participant_signature.0,
					params.participant2_signature.0,
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
