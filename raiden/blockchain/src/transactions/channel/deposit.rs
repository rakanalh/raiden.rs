use std::sync::Arc;

use raiden_primitives::types::{
	Address,
	BlockHash,
	BlockId,
	ChannelIdentifier,
	GasLimit,
	GasPrice,
	TokenAmount,
};
use raiden_state_machine::types::ChannelStatus;
use tokio::sync::RwLockWriteGuard;
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
		TokenProxy,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct ChannelSetTotalDepositTransactionData {
	pub(crate) allowance: TokenAmount,
	pub(crate) current_balance: TokenAmount,
	pub(crate) channel_identifier: ChannelIdentifier,
	pub(crate) amount_to_deposit: TokenAmount,
	pub(crate) channel_onchain_details: ChannelData,
	pub(crate) our_details: ParticipantDetails,
	pub(crate) partner_details: ParticipantDetails,
	pub(crate) network_balance: TokenAmount,
	pub(crate) safety_deprecation_switch: bool,
	pub(crate) token_network_deposit_limit: TokenAmount,
	pub(crate) channel_participant_deposit_limit: TokenAmount,
	pub(crate) network_total_deposit: TokenAmount,
}

#[derive(Clone)]
pub struct ChannelSetTotalDepositTransactionParams {
	pub(crate) channel_identifier: ChannelIdentifier,
	pub(crate) partner: Address,
	pub(crate) total_deposit: TokenAmount,
}

pub struct ChannelSetTotalDepositTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) token_network: TokenNetworkProxy<T>,
	pub(crate) token: TokenProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelSetTotalDepositTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = ();
	type Params = ChannelSetTotalDepositTransactionParams;
	type Data = ChannelSetTotalDepositTransactionData;

	async fn onchain_data(
		&self,
		params: Self::Params,
		at_blockhash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let current_balance =
			self.token.balance_of(self.account.address(), Some(at_blockhash)).await?;

		let channel_identifier = self
			.token_network
			.get_channel_identifier(self.account.address(), params.partner, at_blockhash)
			.await?
			.ok_or(ProxyError::BrokenPrecondition("Block not found".to_string()))?;

		let channel_onchain_details = self
			.token_network
			.channel_details(
				Some(channel_identifier),
				self.account.address(),
				params.partner,
				at_blockhash,
			)
			.await?;

		let our_details = match self
			.token_network
			.participant_details(
				channel_identifier,
				self.account.address(),
				params.partner,
				Some(at_blockhash),
			)
			.await
		{
			Ok(our_details) => our_details,
			Err(_) =>
				self.token_network
					.participant_details(
						channel_identifier,
						self.account.address(),
						params.partner,
						None,
					)
					.await?,
		};

		let partner_details = self
			.token_network
			.participant_details(
				channel_identifier,
				params.partner,
				self.account.address(),
				Some(at_blockhash),
			)
			.await?;

		let allowance = self
			.token
			.allowance(
				self.token_network.contract.address(),
				self.account.address(),
				Some(at_blockhash),
			)
			.await?;

		let network_balance =
			self.token.balance_of(self.account.address(), Some(at_blockhash)).await?;

		let safety_deprecation_switch =
			self.token_network.safety_deprecation_switch(at_blockhash).await?;

		let token_network_deposit_limit =
			self.token_network.token_network_deposit_limit(at_blockhash).await?;

		let channel_participant_deposit_limit =
			self.token_network.channel_participant_deposit_limit(at_blockhash).await?;

		let network_total_deposit =
			self.token.balance_of(self.account.address(), Some(at_blockhash)).await?;

		let amount_to_deposit = params.total_deposit - our_details.deposit;

		Ok(ChannelSetTotalDepositTransactionData {
			allowance,
			current_balance,
			channel_identifier,
			amount_to_deposit,
			channel_onchain_details,
			our_details,
			partner_details,
			network_balance,
			safety_deprecation_switch,
			token_network_deposit_limit,
			channel_participant_deposit_limit,
			network_total_deposit,
		})
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		at_block_hash: BlockHash,
	) -> Result<(), ProxyError> {
		if data.current_balance < data.amount_to_deposit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"new_total_deposit - previous_total_deposit = {} \
                 cannot be larger than the available balance {}.",
				data.amount_to_deposit, data.current_balance,
			)))
		}

		if data.channel_identifier != params.channel_identifier {
			return Err(ProxyError::BrokenPrecondition(format!(
				"There is a channel open between \
                {} and {}. However the channel id \
                on-chain {} and the provided \
                id {} do not match.",
				self.account.address(),
				params.partner,
				params.channel_identifier,
				data.channel_identifier,
			)))
		}

		if data.safety_deprecation_switch {
			return Err(ProxyError::BrokenPrecondition(format!(
				"This token network has been deprecated."
			)))
		}

		if data.channel_onchain_details.status != ChannelStatus::Opened {
			return Err(ProxyError::BrokenPrecondition(format!(
				"The channel was not opened at the provided block \
                ({}). This call should never have been attempted.",
				at_block_hash
			)))
		}

		if params.total_deposit <= data.our_details.deposit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Current total deposit ({}) is already larger \
                than the requested total deposit amount ({})",
				data.our_details.deposit, params.total_deposit,
			)))
		}

		let (_, total_channel_deposit_overflow) =
			params.total_deposit.overflowing_add(data.partner_details.deposit);
		if total_channel_deposit_overflow {
			return Err(ProxyError::BrokenPrecondition(format!("Deposit overflow")))
		}

		if params.total_deposit > data.channel_participant_deposit_limit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Deposit of {} is larger than the \
                channel participant deposit limit",
				params.total_deposit,
			)))
		}

		if data.network_total_deposit + data.amount_to_deposit > data.token_network_deposit_limit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Deposit of {} will have \
                exceeded the token network deposit limit.",
				data.amount_to_deposit,
			)))
		}

		if data.network_balance < data.amount_to_deposit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"new_total_deposit - previous_total_deposit =  {} can \
                not be larger than the available balance {}, \
                for token at address {}",
				data.amount_to_deposit,
				data.network_balance,
				self.account.address(),
			)))
		}

		Ok(())
	}

	async fn execute_prerequisite(
		&self,
		_params: Self::Params,
		data: Self::Data,
	) -> Result<(), ProxyError> {
		self.token
			.approve(
				self.account.clone(),
				self.token_network.contract.address(),
				data.amount_to_deposit,
			)
			.await?;
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

		self.token_network
			.contract
			.signed_call_with_confirmations(
				"setTotalDeposit",
				(
					params.channel_identifier,
					self.account.address(),
					params.total_deposit,
					params.partner,
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
				self.gas_metadata.get("TokenNetwork.setTotalDeposit").into(),
				failed_at_blocknumber,
			)
			.await?;

		let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

		if data.channel_onchain_details.status == ChannelStatus::Closed {
			return Err(ProxyError::Recoverable(format!(
				"Deposit failed because the channel was closed meanwhile",
			)))
		}

		if data.channel_onchain_details.status == ChannelStatus::Settled {
			return Err(ProxyError::Recoverable(format!(
				"Deposit failed because the channel was settled meanwhile",
			)))
		}

		if data.channel_onchain_details.status == ChannelStatus::Removed {
			return Err(ProxyError::Recoverable(format!(
				"Deposit failed because the channel was settled and unlocked meanwhile",
			)))
		}

		let (_, total_channel_deposit_overflow) =
			params.total_deposit.overflowing_add(data.partner_details.deposit);
		if total_channel_deposit_overflow {
			return Err(ProxyError::Recoverable(format!("Deposit overflow")))
		}

		if data.our_details.deposit >= params.total_deposit {
			return Err(ProxyError::Recoverable(format!(
				"Requested total deposit was already performed"
			)))
		}

		if data.network_total_deposit + data.amount_to_deposit > data.token_network_deposit_limit {
			return Err(ProxyError::Recoverable(format!(
				"Deposit of {} will have \
                exceeded the token network deposit limit.",
				data.amount_to_deposit,
			)))
		}

		if params.total_deposit > data.channel_participant_deposit_limit {
			return Err(ProxyError::Recoverable(format!(
				"Deposit of {} is larger than the \
                channel participant deposit limit",
				params.total_deposit,
			)))
		}

		if data.network_balance < data.amount_to_deposit {
			return Err(ProxyError::Recoverable(format!(
				"new_total_deposit - previous_total_deposit =  {} can \
                not be larger than the available balance {}, \
                for token at address {}",
				data.amount_to_deposit,
				data.network_balance,
				self.account.address(),
			)))
		}

		let has_sufficient_balance = self
			.token
			.balance_of(self.token_network.contract.address(), Some(failed_at_blockhash))
			.await? >= data.amount_to_deposit;
		if !has_sufficient_balance {
			return Err(ProxyError::Recoverable(format!(
				"The account does not have enough balance to complete the deposit"
			)))
		}

		if data.allowance < data.amount_to_deposit {
			return Err(ProxyError::Recoverable(format!(
				"The allowance of the {} deposit changed, current: {}. \
                Check concurrent deposits \
                for the same token network but different proxies.",
				data.amount_to_deposit, data.allowance,
			)))
		}

		let latest_deposit = self
			.token_network
			.participant_details(
				params.channel_identifier,
				self.account.address(),
				params.partner,
				Some(failed_at_blockhash),
			)
			.await?
			.deposit;
		if latest_deposit < params.total_deposit {
			return Err(ProxyError::Recoverable(format!("The tokens were not transferred")))
		}

		return Err(ProxyError::Recoverable(format!("deposit failed for an unknown reason")))
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
				"setTotalDeposit",
				(
					params.channel_identifier,
					self.account.address(),
					params.total_deposit,
					params.partner,
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

	async fn acquire_lock(&self) -> Option<RwLockWriteGuard<bool>> {
		Some(self.token.lock.write().await)
	}
}
