use std::{
	ops::{
		Mul,
		Sub,
	},
	sync::Arc,
};

use raiden_blockchain::{
	errors::ContractDefError,
	proxies::{
		Account,
		GasReserve,
		ProxyError,
	},
};
use raiden_pathfinding::{
	query_address_metadata,
	routing,
	RoutingError,
};
use raiden_primitives::{
	hashing::hash_secret,
	payments::{
		PaymentStatus,
		PaymentsRegistry,
	},
	traits::Checksum,
	types::{
		Address,
		BlockTimeout,
		Bytes,
		CanonicalIdentifier,
		ChannelIdentifier,
		PaymentIdentifier,
		RetryTimeout,
		RevealTimeout,
		Secret,
		SecretHash,
		SecretRegistryAddress,
		SettleTimeout,
		TokenAddress,
		TokenAmount,
		TokenNetworkAddress,
		TokenNetworkRegistryAddress,
		TransactionHash,
	},
};
use raiden_state_machine::{
	constants::{
		ABSENT_SECRET,
		DEFAULT_RETRY_TIMEOUT,
		MIN_REVEAL_TIMEOUT,
		SECRET_LENGTH,
	},
	errors::StateTransitionError,
	types::{
		ActionChannelClose,
		ActionChannelCoopSettle,
		ActionChannelSetRevealTimeout,
		ActionChannelWithdraw,
		ActionInitInitiator,
		ChannelState,
		ChannelStatus,
		RouteState,
		StateChange,
		TransferDescriptionWithSecretState,
	},
	views,
};
use raiden_transition::Transitioner;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{
	debug,
	error,
	info,
};
use web3::transports::Http;

use crate::{
	raiden::Raiden,
	utils::{
		random_identifier,
		random_secret,
	},
	waiting,
};

#[derive(Error, Debug)]
pub enum ContractError {}

#[derive(Error, Debug)]
pub enum ApiError {
	#[error("Transition Error: `{0}`")]
	Transition(StateTransitionError),
	#[error("Contract definition error: `{0}`")]
	ContractSpec(ContractDefError),
	#[error("Contract error: `{0}`")]
	Contract(web3::contract::Error),
	#[error("Proxy error: `{0}`")]
	Proxy(ProxyError),
	#[error("Web3 error: `{0}`")]
	Web3(web3::Error),
	#[error("On-chain error: `{0}`")]
	OnChainCall(String),
	#[error("Invalid state: `{0}`")]
	State(String),
	#[error("Routing error: `{0}`")]
	Routing(RoutingError),
	#[error("Invalid parameter: `{0}`")]
	Param(String),
}

pub struct Payment {
	pub target: Address,
	pub payment_identifier: PaymentIdentifier,
	pub secret: Secret,
	pub secrethash: SecretHash,
}

pub struct Api {
	pub raiden: Arc<Raiden>,
	transition_service: Arc<Transitioner>,
	payments_registry: Arc<RwLock<PaymentsRegistry>>,
}

impl Api {
	pub fn new(
		raiden: Arc<Raiden>,
		transition_service: Arc<Transitioner>,
		payments_registry: Arc<RwLock<PaymentsRegistry>>,
	) -> Self {
		Self { raiden, transition_service, payments_registry }
	}

	pub async fn create_channel(
		&self,
		account: Account<Http>,
		registry_address: Address,
		token_address: TokenAddress,
		partner_address: Address,
		settle_timeout: Option<SettleTimeout>,
		reveal_timeout: Option<RevealTimeout>,
		retry_timeout: Option<RetryTimeout>,
	) -> Result<ChannelIdentifier, ApiError> {
		let current_state = &self.raiden.state_manager.read().current_state.clone();

		info!(
			message = "Opening channel.",
			registry_address = registry_address.checksum(),
			partner_address = partner_address.checksum(),
			token_address = token_address.checksum(),
			settle_timeout = settle_timeout.map(|t| t.to_string()),
			reveal_timeout = reveal_timeout.map(|t| t.to_string()),
		);
		let settle_timeout = settle_timeout.unwrap_or(self.raiden.config.default_settle_timeout);
		let reveal_timeout = reveal_timeout.unwrap_or(self.raiden.config.default_reveal_timeout);

		self.check_invalid_channel_timeouts(settle_timeout, reveal_timeout)?;

		let confirmed_block_identifier = current_state.block_hash;
		let registry = self
			.raiden
			.proxy_manager
			.token_network_registry(registry_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let settlement_timeout_min = registry
			.settlement_timeout_min(confirmed_block_identifier)
			.await
			.map_err(ApiError::Proxy)?;
		let settlement_timeout_max = registry
			.settlement_timeout_max(confirmed_block_identifier)
			.await
			.map_err(ApiError::Proxy)?;

		if settle_timeout < settlement_timeout_min {
			return Err(ApiError::Param(format!(
				"Settlement timeout should be at least {}",
				settlement_timeout_min,
			)))
		}

		if settle_timeout > settlement_timeout_max {
			return Err(ApiError::Param(format!(
				"Settlement timeout exceeds max of {}",
				settlement_timeout_max,
			)))
		}

		let token_network_address = registry
			.get_token_network(token_address, confirmed_block_identifier)
			.await
			.map_err(ApiError::Proxy)?;

		if token_network_address.is_zero() {
			return Err(ApiError::Param(format!(
				"Token network for token {} does not exist",
				token_address,
			)))
		}

		let mut token_network = self
			.raiden
			.proxy_manager
			.token_network(token_address, token_network_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let safety_deprecation_switch = token_network
			.safety_deprecation_switch(confirmed_block_identifier)
			.await
			.map_err(ApiError::Proxy)?;

		if safety_deprecation_switch {
			return Err(ApiError::OnChainCall(
				"This token_network has been deprecated. New channels cannot be
                open for this network, usage of the newly deployed token
                network contract is highly encouraged."
					.to_owned(),
			))
		}

		let duplicated_channel = token_network
			.get_channel_identifier(
				token_network_address,
				partner_address,
				Some(confirmed_block_identifier),
			)
			.await
			.map_err(ApiError::Proxy)?;

		if duplicated_channel.is_some() {
			return Err(ApiError::OnChainCall(format!(
				"A channel with {} for token
                {} already exists.
                (At blockhash: {})",
				partner_address, token_address, confirmed_block_identifier,
			)))
		}

		let chain_state = &self.raiden.state_manager.read().current_state.clone();
		let gas_reserve = GasReserve::new(self.raiden.proxy_manager.clone(), registry_address);
		let (has_enough_reserve, estimated_required_reserve) = gas_reserve
			.has_enough(account.clone(), chain_state, 1)
			.await
			.map_err(ApiError::Proxy)?;

		if !has_enough_reserve {
			return Err(ApiError::OnChainCall(format!(
				"The account balance is below the estimated amount necessary to \
                finish the lifecycles of all active channels. A balance of at \
                least {} wei is required.",
				estimated_required_reserve,
			)))
		}

		let channel_identifier = match token_network
			.new_channel(
				account.clone(),
				partner_address,
				settle_timeout,
				confirmed_block_identifier,
			)
			.await
		{
			Ok(channel_identifier) => channel_identifier,
			Err(e) => {
				// Check if channel has already been created by partner
				if let Ok(Some(channel_ideitifier)) = token_network
					.get_channel_identifier(account.address(), partner_address, None)
					.await
				{
					channel_ideitifier
				} else {
					return Err(ApiError::Proxy(e))
				}
			},
		};

		waiting::wait_for_new_channel(
			self.raiden.state_manager.clone(),
			registry_address,
			token_address,
			partner_address,
			retry_timeout,
		)
		.await?;

		let chain_state = &self.raiden.state_manager.read().current_state.clone();
		let channel_state = match views::get_channel_state_for(
			chain_state,
			registry_address,
			token_address,
			partner_address,
		) {
			Some(channel_state) => channel_state,
			None => return Err(ApiError::State(format!("Channel was not found"))),
		};

		debug!(
			message = "Channel opened",
			channel_identifier = channel_state.canonical_identifier.channel_identifier.to_string()
		);

		if let Err(e) = self
			.transition_service
			.transition(vec![ActionChannelSetRevealTimeout {
				canonical_identifier: channel_state.canonical_identifier.clone(),
				reveal_timeout,
			}
			.into()])
			.await
		{
			return Err(ApiError::State(e))
		}

		Ok(channel_identifier)
	}

	pub async fn update_channel(
		&self,
		account: Account<Http>,
		registry_address: Address,
		token_address: TokenAddress,
		partner_address: Address,
		reveal_timeout: Option<RevealTimeout>,
		total_deposit: Option<TokenAmount>,
		total_withdraw: Option<TokenAmount>,
		state: Option<ChannelStatus>,
		retry_timeout: Option<RetryTimeout>,
	) -> Result<(), ApiError> {
		info!(
			message = "Patching channel.",
			registry_address = registry_address.checksum(),
			partner_address = partner_address.checksum(),
			token_address = token_address.checksum(),
			reveal_timeout = reveal_timeout.map(|t| t.to_string()),
			total_deposit = total_deposit.map(|t| t.to_string()),
			total_withdraw = total_withdraw.map(|t| t.to_string()),
			state = state.map(|t| t.to_string()),
		);

		if reveal_timeout.is_some() && state.is_some() {
			return Err(ApiError::Param(format!(
				"Can not update a channel's reveal timeout and state at the same time",
			)))
		}

		if total_deposit.is_some() && state.is_some() {
			return Err(ApiError::Param(format!(
				"Can not update a channel's total deposit and state at the same time",
			)))
		}

		if total_withdraw.is_some() && state.is_some() {
			return Err(ApiError::Param(format!(
				"Can not update a channel's total withdraw and state at the same time",
			)))
		}

		if total_withdraw.is_some() && total_deposit.is_some() {
			return Err(ApiError::Param(format!(
				"Can not update a channel's total withdraw and total deposit at the same time",
			)))
		}

		if reveal_timeout.is_some() && total_deposit.is_some() {
			return Err(ApiError::Param(format!(
				"Can not update a channel's reveal timeout and total deposit at the same time",
			)))
		}

		if reveal_timeout.is_some() && total_withdraw.is_some() {
			return Err(ApiError::Param(format!(
				"Can not update a channel's reveal timeout and total withdraw at the same time",
			)))
		}

		if let Some(total_deposit) = total_deposit {
			if total_deposit < TokenAmount::zero() {
				return Err(ApiError::Param(format!("Amount to deposit must not be negative")))
			}
		}

		if let Some(total_withdraw) = total_withdraw {
			if total_withdraw < TokenAmount::zero() {
				return Err(ApiError::Param(format!("Amount to withdraw must not be negative")))
			}
		}

		let empty_request = total_deposit.is_none() &&
			state.is_none() &&
			total_withdraw.is_none() &&
			reveal_timeout.is_none();

		if empty_request {
			return Err(ApiError::Param(format!(
				"Nothing to do. Should either provide \
                `total_deposit, `total_withdraw`, `reveal_timeout` or `state` argument"
			)))
		}

		let current_state = &self.raiden.state_manager.read().current_state.clone();
		let channel_state = match views::get_channel_state_for(
			current_state,
			registry_address,
			token_address,
			partner_address,
		) {
			Some(channel_state) => channel_state,
			None =>
				return Err(ApiError::State(format!(
					"Requested channel for token {} and partner {} not found",
					token_address, partner_address,
				))),
		};

		let result = if let Some(total_deposit) = total_deposit {
			self.channel_deposit(account, channel_state, total_deposit, retry_timeout).await
		} else if let Some(total_withdraw) = total_withdraw {
			self.channel_withdraw(channel_state, total_withdraw).await
		} else if let Some(reveal_timeout) = reveal_timeout {
			self.channel_reveal_timeout(channel_state, reveal_timeout).await
		} else if let Some(state) = state {
			if state == ChannelStatus::Closed {
				return self.channel_close(registry_address, channel_state).await
			}
			return Err(ApiError::Param(format!("Unreachable")))
		} else {
			return Err(ApiError::Param(format!("Unreachable")))
		};

		result
	}

	pub async fn channel_deposit(
		&self,
		account: Account<Http>,
		channel_state: &ChannelState,
		total_deposit: TokenAmount,
		retry_timeout: Option<RetryTimeout>,
	) -> Result<(), ApiError> {
		info!(
			message = "Depositing to channel.",
			channel_identifier = channel_state.canonical_identifier.channel_identifier.to_string(),
			total_deposit = total_deposit.to_string(),
		);

		if channel_state.status() != ChannelStatus::Opened {
			return Err(ApiError::State(format!("Can't set total deposit on a closed channel")))
		}

		let chain_state = &self.raiden.state_manager.read().current_state.clone();
		let confirmed_block_identifier = chain_state.block_hash;
		let token = self
			.raiden
			.proxy_manager
			.token(channel_state.token_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let token_network_registry = self
			.raiden
			.proxy_manager
			.token_network_registry(channel_state.token_network_registry_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let token_network_address = token_network_registry
			.get_token_network(channel_state.token_address, confirmed_block_identifier)
			.await
			.map_err(ApiError::Proxy)?;

		let token_network_proxy = self
			.raiden
			.proxy_manager
			.token_network(channel_state.token_address, token_network_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let channel_proxy = self
			.raiden
			.proxy_manager
			.payment_channel(channel_state)
			.await
			.map_err(ApiError::ContractSpec)?;

		let blockhash = chain_state.block_hash;

		let safety_deprecation_switch = token_network_proxy
			.safety_deprecation_switch(blockhash)
			.await
			.map_err(ApiError::Proxy)?;

		let balance = token
			.balance_of(chain_state.our_address, Some(blockhash))
			.await
			.map_err(ApiError::Proxy)?;

		let network_balance = token
			.balance_of(token_network_address, Some(blockhash))
			.await
			.map_err(ApiError::Proxy)?;

		let token_network_deposit_limit = token_network_proxy
			.token_network_deposit_limit(blockhash)
			.await
			.map_err(ApiError::Proxy)?;

		let channel_participant_deposit_limit = token_network_proxy
			.channel_participant_deposit_limit(blockhash)
			.await
			.map_err(ApiError::Proxy)?;

		let (_, total_channel_deposit_overflow) =
			total_deposit.overflowing_add(channel_state.partner_state.contract_balance);

		if safety_deprecation_switch {
			return Err(ApiError::State(format!(
				"This token_network has been deprecated. \
                All channels in this network should be closed and \
                the usage of the newly deployed token network contract \
                is highly encouraged."
			)))
		}

		if total_deposit <= channel_state.our_state.contract_balance {
			return Err(ApiError::State(format!("Total deposit did not increase.")))
		}

		if total_deposit < channel_state.our_state.contract_balance {
			return Err(ApiError::State(format!(
				"The new total deposit {:?} is less than the current total deposit {:?}",
				total_deposit, channel_state.our_state.contract_balance,
			)))
		}

		let deposit_increase = total_deposit - channel_state.our_state.contract_balance;
		// If this check succeeds it does not imply the `deposit` will
		// succeed, since the `deposit` transaction may race with another
		// transaction.
		if balance < deposit_increase {
			return Err(ApiError::State(format!(
				"Not enough balance to deposit. Available={} Needed={}",
				balance, deposit_increase,
			)))
		}

		if network_balance + deposit_increase > token_network_deposit_limit {
			return Err(ApiError::State(format!(
				"Deposit of {} would have exceeded \
                the token network deposit limit.",
				deposit_increase,
			)))
		}

		if total_deposit > channel_participant_deposit_limit {
			return Err(ApiError::State(format!(
				"Deposit of {} is larger than the \
                channel participant deposit limit",
				total_deposit,
			)))
		}

		if total_channel_deposit_overflow {
			return Err(ApiError::State(format!("Deposit overflow",)))
		}

		channel_proxy
			.approve_and_set_total_deposit(
				account.clone(),
				channel_state.canonical_identifier.channel_identifier,
				channel_state.partner_state.address,
				total_deposit,
				blockhash,
			)
			.await
			.map_err(ApiError::Proxy)?;

		waiting::wait_for_participant_deposit(
			self.raiden.state_manager.clone(),
			channel_state.token_network_registry_address,
			channel_state.token_address,
			channel_state.partner_state.address,
			channel_state.our_state.address,
			total_deposit,
			retry_timeout,
		)
		.await?;

		Ok(())
	}

	pub async fn channel_withdraw(
		&self,
		channel_state: &ChannelState,
		total_withdraw: TokenAmount,
	) -> Result<(), ApiError> {
		info!(
			message = "Withdraw from channel.",
			channel_identifier = channel_state.canonical_identifier.channel_identifier.to_string(),
			total_withdraw = total_withdraw.to_string(),
		);
		if channel_state.status() != ChannelStatus::Opened {
			return Err(ApiError::State(format!("Can't withdraw from a closed channel")))
		}

		let current_balance =
			views::channel_balance(&channel_state.our_state, &channel_state.partner_state);
		let amount_to_withdraw = total_withdraw.sub(channel_state.our_total_withdraw());
		if amount_to_withdraw > current_balance {
			return Err(ApiError::State(format!(
				"The withdraw of {} is bigger than the current balance of {}",
				amount_to_withdraw, current_balance
			)))
		}

		let recipient_address = channel_state.partner_state.address;
		let recipient_metadata = match query_address_metadata(
			self.raiden.config.pfs_config.url.clone(),
			recipient_address,
		)
		.await
		{
			Ok(metadata) => metadata,
			Err(e) => {
				error!(
					message = "Could not retrieve partner's address metadata",
					address = recipient_address.checksum(),
					error = format!("{:?}", e),
				);
				return Err(ApiError::State(format!(
					"Could not retrieve partner's address metadata"
				)))
			},
		};

		let state_change = ActionChannelWithdraw {
			canonical_identifier: channel_state.canonical_identifier.clone(),
			total_withdraw,
			recipient_metadata: Some(recipient_metadata),
		};

		if let Err(e) = self.transition_service.transition(vec![state_change.into()]).await {
			error!(message = format!("{:?}", e));
			return Err(ApiError::State(format!("{:?}", e)))
		}

		if let Err(e) = waiting::wait_for_withdraw_complete(
			self.raiden.state_manager.clone(),
			channel_state.canonical_identifier.clone(),
			total_withdraw,
			Some(DEFAULT_RETRY_TIMEOUT),
		)
		.await
		{
			error!(message = format!("{:?}", e));
			return Err(ApiError::State(format!("{:?}", e)))
		}

		Ok(())
	}

	pub async fn channel_reveal_timeout(
		&self,
		channel_state: &ChannelState,
		reveal_timeout: RevealTimeout,
	) -> Result<(), ApiError> {
		info!(
			message = "Set reveal timeout for channel.",
			channel_identifier = channel_state.canonical_identifier.channel_identifier.to_string(),
			reveal_timeout = reveal_timeout.to_string(),
		);
		if channel_state.status() != ChannelStatus::Opened {
			return Err(ApiError::State(format!(
				"Can't update the reveal timeout of a closed channel"
			)))
		}

		if channel_state.settle_timeout < reveal_timeout.mul(2) {
			return Err(ApiError::State(format!(
				"`settle_timeout` can not be smaller than double the \
                `reveal_timeout`.\n \
                The setting `reveal_timeout` determines the maximum number of \
                blocks it should take a transaction to be mined when the \
                blockchain is under congestion. This setting determines the \
                when a node must go on-chain to register a secret, and it is \
                therefore the lower bound of the lock expiration. The \
                `settle_timeout` determines when a channel can be settled \
                on-chain, for this operation to be safe all locks must have \
                been resolved, for this reason the `settle_timeout` has to be \
                larger than `reveal_timeout`."
			)))
		}

		let state_change = ActionChannelSetRevealTimeout {
			canonical_identifier: channel_state.canonical_identifier.clone(),
			reveal_timeout,
		};

		if let Err(e) = self.transition_service.transition(vec![state_change.into()]).await {
			error!(message = format!("{:?}", e));
			return Err(ApiError::State(format!("{:?}", e)))
		}

		Ok(())
	}

	pub async fn channel_close(
		&self,
		registry_address: Address,
		channel_state: &ChannelState,
	) -> Result<(), ApiError> {
		info!(
			message = "Close channel.",
			channel_identifier = channel_state.canonical_identifier.channel_identifier.to_string(),
		);
		if channel_state.status() != ChannelStatus::Opened {
			return Err(ApiError::State(format!("Attempted to close an already closed channel")))
		}
		self.channel_batch_close(
			registry_address,
			channel_state.token_address,
			vec![channel_state.partner_state.address],
			Some(DEFAULT_RETRY_TIMEOUT),
			true,
		)
		.await
	}

	pub async fn token_network_register(
		&self,
		registry_address: Address,
		token_address: TokenAddress,
	) -> Result<TokenNetworkAddress, ApiError> {
		info!(
			message = "Register token network.",
			registry_address = registry_address.checksum(),
			token_address = token_address.checksum(),
		);
		if token_address == TokenAddress::zero() {
			return Err(ApiError::Param(format!("Token address must be non-zero")))
		}

		let chain_state = self.raiden.state_manager.read().current_state.clone();
		let tokens_list = views::get_token_identifiers(&chain_state, registry_address);
		if tokens_list.contains(&token_address) {
			return Err(ApiError::Param(format!("Token already registered")))
		}

		let token_proxy = self
			.raiden
			.proxy_manager
			.token(token_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let token_network_registry = self
			.raiden
			.proxy_manager
			.token_network_registry(registry_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let (_, token_network_address) = token_network_registry
			.add_token(
				self.raiden.config.account.clone(),
				token_proxy,
				token_address,
				chain_state.block_hash,
			)
			.await
			.map_err(ApiError::Proxy)?;

		if let Err(e) = waiting::wait_for_token_network(
			self.raiden.state_manager.clone(),
			token_network_address,
			token_address,
			Some(DEFAULT_RETRY_TIMEOUT),
		)
		.await
		{
			error!(message = format!("{:?}", e));
			return Err(ApiError::State(format!("{:?}", e)))
		}
		Ok(token_network_address)
	}

	pub async fn token_network_leave(
		&self,
		registry_address: Address,
		token_address: TokenAddress,
	) -> Result<Vec<ChannelState>, ApiError> {
		info!(
			message = "Leave token network.",
			registry_address = registry_address.checksum(),
			token_address = token_address.checksum(),
		);
		let chain_state = self.raiden.state_manager.read().current_state.clone();
		let channels: Vec<ChannelState> = match views::get_token_network_by_token_address(
			&chain_state,
			registry_address,
			token_address,
		)
		.map(|t| t.channelidentifiers_to_channels.values().cloned().collect())
		{
			Some(channels) => channels,
			None =>
				return Err(ApiError::State(format!(
					"Token {} is not registered with network {}",
					token_address.checksum(),
					registry_address.checksum()
				))),
		};

		self.channel_batch_close(
			registry_address,
			token_address,
			channels.iter().map(|c| c.partner_state.address.clone()).collect(),
			Some(DEFAULT_RETRY_TIMEOUT),
			true,
		)
		.await?;
		Ok(channels)
	}

	pub async fn channel_batch_close(
		&self,
		registry_address: Address,
		token_address: TokenAddress,
		partners: Vec<Address>,
		retry_timeout: Option<RetryTimeout>,
		coop_settle: bool,
	) -> Result<(), ApiError> {
		let chain_state = self.raiden.state_manager.read().current_state.clone();
		let valid_tokens = views::get_token_identifiers(&chain_state, registry_address);
		if !valid_tokens.contains(&token_address) {
			return Err(ApiError::State("Token address is not known".to_owned()))
		}
		let channels_to_close = views::filter_channels_by_partner_address(
			&chain_state,
			registry_address,
			token_address,
			partners,
		);

		if coop_settle {
			if let Err(e) = self.batch_coop_settle(channels_to_close.clone(), retry_timeout).await {
				error!(message = format!("{:?}.. skipping cooperative settle", e));
			}
		}

		let canonical_ids =
			channels_to_close.iter().map(|c| c.canonical_identifier.clone()).collect();

		let close_state_changes = channels_to_close
			.iter()
			.map(|c| {
				ActionChannelClose { canonical_identifier: c.canonical_identifier.clone() }.into()
			})
			.collect();

		if let Err(e) = self.transition_service.transition(close_state_changes).await {
			error!(message = format!("{:?}", e));
			return Err(ApiError::State(format!("{:?}", e)))
		}

		if let Err(e) =
			waiting::wait_for_close(self.raiden.state_manager.clone(), canonical_ids, retry_timeout)
				.await
		{
			error!(message = format!("{:?}", e));
			return Err(ApiError::State(format!("{:?}", e)))
		}

		Ok(())
	}

	pub async fn batch_coop_settle(
		&self,
		channels: Vec<&ChannelState>,
		retry_timeout: Option<RetryTimeout>,
	) -> Result<Vec<ChannelState>, ApiError> {
		let mut coop_settle_state_changes: Vec<StateChange> = vec![];
		for channel in channels.iter() {
			let recipient_address = channel.partner_state.address;
			let recipient_metadata = match query_address_metadata(
				self.raiden.config.pfs_config.url.clone(),
				recipient_address,
			)
			.await
			{
				Ok(metadata) => metadata,
				Err(e) => {
					error!(
						message = "Partner is offline, coop settle is not possible",
						address = recipient_address.checksum(),
						error = format!("{:?}", e),
					);
					continue
				},
			};
			coop_settle_state_changes.push(
				ActionChannelCoopSettle {
					canonical_identifier: channel.canonical_identifier.clone(),
					recipient_metadata: Some(recipient_metadata),
				}
				.into(),
			);
		}

		if coop_settle_state_changes.is_empty() {
			return Ok(vec![])
		}

		if let Err(e) = self.transition_service.transition(coop_settle_state_changes).await {
			error!(message = format!("{:?}", e));
			return Err(ApiError::State(format!("{:?}", e)))
		}

		let settling_channel_ids: Vec<CanonicalIdentifier> =
			channels.iter().map(|c| c.canonical_identifier.clone()).collect();

		let chain_state = self.raiden.state_manager.read().current_state.clone();

		let mut channels_to_settle: Vec<CanonicalIdentifier> = vec![];
		for channel_canonical_id in settling_channel_ids {
			if let Some(channel_to_settle) =
				views::get_channel_by_canonical_identifier(&chain_state, channel_canonical_id)
			{
				if channel_to_settle.our_state.initiated_coop_settle.is_none() {
					continue
				}

				channels_to_settle.push(channel_to_settle.canonical_identifier.clone());
			};
		}

		if let Err(e) = waiting::wait_for_coop_settle(
			self.raiden.web3.clone(),
			self.raiden.state_manager.clone(),
			channels_to_settle.clone(),
			retry_timeout,
		)
		.await
		{
			error!(message = format!("{:?}", e));
			return Err(ApiError::State(format!("{:?}", e)))
		}

		let chain_state = self.raiden.state_manager.read().current_state.clone();
		let mut unsuccessful_channels: Vec<ChannelState> = vec![];
		for canonical_identifier in channels_to_settle {
			if let Some(new_channel_state) =
				views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier)
			{
				if new_channel_state.status() != ChannelStatus::Settled {
					unsuccessful_channels.push(new_channel_state.clone());
				}
			}
		}
		Ok(unsuccessful_channels)
	}

	pub async fn deposit_to_udc(
		&self,
		user_deposit_address: Address,
		new_total_deposit: TokenAmount,
	) -> Result<(), ApiError> {
		info!(
			message = "Deposit to UDC",
			user_deposit_address = user_deposit_address.checksum(),
			new_total_deposit = new_total_deposit.to_string(),
		);
		let user_deposit_proxy = self
			.raiden
			.proxy_manager
			.user_deposit(user_deposit_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let confirmed_block_identifier =
			self.raiden.state_manager.read().current_state.block_hash.clone();

		let current_total_deposit = user_deposit_proxy
			.total_deposit(self.raiden.config.account.address(), Some(confirmed_block_identifier))
			.await
			.map_err(ApiError::Proxy)?;

		let deposit_increase = new_total_deposit - current_total_deposit;

		let whole_balance = user_deposit_proxy
			.whole_balance(Some(confirmed_block_identifier))
			.await
			.map_err(ApiError::Proxy)?;

		let whole_balance_limit = user_deposit_proxy
			.whole_balance_limit(Some(confirmed_block_identifier))
			.await
			.map_err(ApiError::Proxy)?;

		let token_address = user_deposit_proxy
			.token_address(Some(confirmed_block_identifier))
			.await
			.map_err(ApiError::Proxy)?;

		let token_proxy = self
			.raiden
			.proxy_manager
			.token(token_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let balance = token_proxy
			.balance_of(self.raiden.config.account.address(), Some(confirmed_block_identifier))
			.await
			.map_err(ApiError::Proxy)?;

		if new_total_deposit <= current_total_deposit {
			return Err(ApiError::Param(format!("Total deposit did not increase")))
		}

		if whole_balance.checked_add(deposit_increase).is_none() {
			return Err(ApiError::Param(format!("Deposit overflow")))
		}

		if whole_balance.saturating_add(deposit_increase) > whole_balance_limit {
			return Err(ApiError::Param(format!(
				"Deposit of {:?} would have exceeded the UDC balance limit",
				deposit_increase
			)))
		}

		if balance < deposit_increase {
			return Err(ApiError::Param(format!(
				"Not enough balance to deposit. Available: {:?}, Needed: {:?}",
				balance, deposit_increase
			)))
		}

		if let Err(e) = user_deposit_proxy
			.deposit(
				self.raiden.config.account.clone(),
				token_proxy,
				new_total_deposit,
				confirmed_block_identifier,
			)
			.await
		{
			error!("Failed to set a new total deposit for UDC: {:?}", e);
			return Err(ApiError::Proxy(e))
		}
		Ok(())
	}

	pub async fn plan_withdraw_from_udc(
		&self,
		user_deposit_address: Address,
		planned_withdraw_amount: TokenAmount,
	) -> Result<(), ApiError> {
		info!(
			message = "Plan withdraw from UDC",
			user_deposit_address = user_deposit_address.checksum(),
			planned_withdraw_amount = planned_withdraw_amount.to_string(),
		);
		let user_deposit_proxy = self
			.raiden
			.proxy_manager
			.user_deposit(user_deposit_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let confirmed_block_identifier =
			self.raiden.state_manager.read().current_state.block_hash.clone();

		let balance = user_deposit_proxy
			.balance(self.raiden.config.account.address(), Some(confirmed_block_identifier))
			.await
			.map_err(ApiError::Proxy)?;

		if planned_withdraw_amount == TokenAmount::zero() {
			return Err(ApiError::Param(format!("Withdraw amount must be greater than zero")))
		}

		if planned_withdraw_amount > balance {
			return Err(ApiError::State(format!(
				"The withdraw amount of {} is bigger than the current balance of {}",
				planned_withdraw_amount, balance
			)))
		}

		if let Err(e) = user_deposit_proxy
			.plan_withdraw(
				self.raiden.config.account.clone(),
				planned_withdraw_amount,
				confirmed_block_identifier,
			)
			.await
		{
			error!("Failed to set a new total deposit for UDC: {:?}", e);
			return Err(ApiError::Proxy(e))
		}

		Ok(())
	}

	pub async fn withdraw_from_udc(
		&self,
		user_deposit_address: Address,
		withdraw_amount: TokenAmount,
	) -> Result<(), ApiError> {
		info!(
			message = "Withdraw from UDC",
			user_deposit_address = user_deposit_address.checksum(),
			withdraw_amount = withdraw_amount.to_string(),
		);
		let user_deposit_proxy = self
			.raiden
			.proxy_manager
			.user_deposit(user_deposit_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let chain_state = self.raiden.state_manager.read().current_state.clone();
		let confirmed_block_identifier = chain_state.block_hash.clone();
		let block_number = chain_state.block_number.clone();
		drop(chain_state);

		let withdraw_plan = user_deposit_proxy
			.withdraw_plan(self.raiden.config.account.address(), Some(confirmed_block_identifier))
			.await
			.map_err(ApiError::Proxy)?;

		let whole_balance = user_deposit_proxy
			.whole_balance(Some(confirmed_block_identifier))
			.await
			.map_err(ApiError::Proxy)?;

		if withdraw_amount.is_zero() {
			return Err(ApiError::Param(format!("Withdraw amount must be greater than zero",)))
		}

		if withdraw_amount > withdraw_plan.withdraw_amount {
			return Err(ApiError::Param(format!("Withdraw more than planned")))
		}

		if block_number < withdraw_plan.withdraw_block {
			return Err(ApiError::Param(format!(
				"Withdrawing too early. Planned withdraw at block: {}, current_block {}",
				withdraw_plan.withdraw_block, confirmed_block_identifier
			)))
		}

		if whole_balance.checked_sub(withdraw_amount).is_none() {
			return Err(ApiError::Param(format!("Whole balance underflow")))
		}

		if let Err(e) = user_deposit_proxy
			.withdraw(
				self.raiden.config.account.clone(),
				withdraw_amount,
				confirmed_block_identifier,
			)
			.await
		{
			error!("Failed to set a new total deposit for UDC: {:?}", e);
			return Err(ApiError::Proxy(e))
		}

		Ok(())
	}

	pub async fn initiate_payment(
		&self,
		account: Account<Http>,
		token_network_registry_address: TokenNetworkRegistryAddress,
		secret_registry_address: SecretRegistryAddress,
		token_address: TokenAddress,
		partner_address: Address,
		amount: TokenAmount,
		payment_identifier: Option<PaymentIdentifier>,
		secret: Option<String>,
		secret_hash: Option<SecretHash>,
		lock_timeout: Option<BlockTimeout>,
	) -> Result<Payment, ApiError> {
		info!(
			message = "Initiate payment",
			token_address = token_address.checksum(),
			partner_address = partner_address.checksum(),
			amount = amount.to_string(),
		);
		if account.address() == partner_address {
			return Err(ApiError::Param(format!("Address must be different for partner")))
		}

		if amount == TokenAmount::zero() {
			return Err(ApiError::Param(format!("Amount should not be zero")))
		}

		let chain_state = &self.raiden.state_manager.read().current_state.clone();
		let valid_tokens =
			views::get_token_identifiers(chain_state, token_network_registry_address);
		if !valid_tokens.contains(&token_address) {
			return Err(ApiError::Param(format!("Token address is not known")))
		}

		let payment_identifier = match payment_identifier {
			Some(identifier) => identifier,
			None => random_identifier(),
		};

		let token_network = views::get_token_network_by_token_address(
			chain_state,
			token_network_registry_address,
			token_address,
		)
		.ok_or(ApiError::Param(format!(
			"Token {} is not registered with network {}",
			token_address, token_network_registry_address
		)))?;
		let token_network_address = token_network.address;

		let secret = match secret {
			Some(secret) => Bytes(secret.as_bytes().to_vec()),
			None =>
				if secret_hash.is_none() {
					Bytes(random_secret().as_bytes().to_vec())
				} else {
					ABSENT_SECRET
				},
		};

		let secret_hash = match secret_hash {
			Some(hash) => hash,
			None => SecretHash::from_slice(&hash_secret(&secret.0)),
		};

		if !secret.0.is_empty() {
			let secrethash_from_secret = SecretHash::from_slice(&hash_secret(&secret.0));
			if secret_hash != secrethash_from_secret {
				return Err(ApiError::Param(format!("Provided secret and secret_hash do not match")))
			}
		}

		if secret.0.len() != SECRET_LENGTH as usize {
			return Err(ApiError::Param(format!("Secret of invalid length")))
		}

		let secret_registry_proxy = self
			.raiden
			.proxy_manager
			.secret_registry(secret_registry_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let secret_registered = secret_registry_proxy
			.is_secret_registered(secret_hash, None)
			.await
			.map_err(ApiError::Proxy)?;

		if secret_registered {
			return Err(ApiError::Param(format!(
				"Attempted to initiate a locked transfer with secrethash
                `{}`. That secret is already registered onchain",
				secret_hash,
			)))
		}

		if let Some(payment) =
			self.payments_registry.read().await.get(partner_address, payment_identifier)
		{
			let matches =
				payment.token_network_address == token_network_address && payment.amount == amount;
			if matches {
				return Err(ApiError::Param(format!(
					"Another payment with the same id is in flight"
				)))
			}
		}

		let payment_completed = self.payments_registry.write().await.register(
			token_network_address,
			partner_address,
			payment_identifier,
			amount,
		);

		let action_initiator_init = self
			.initiator_init(
				payment_identifier,
				amount,
				secret.clone(),
				secret_hash,
				token_network_registry_address,
				token_network_address,
				partner_address,
				lock_timeout,
				None,
			)
			.await;

		match action_initiator_init {
			Ok(action_init_initiator) => {
				if let Err(e) =
					self.transition_service.transition(vec![action_init_initiator.into()]).await
				{
					error!("{}", e);
					return Err(ApiError::State(e))
				}
			},
			Err(e) => {
				self.payments_registry.write().await.complete(PaymentStatus::Error(
					partner_address,
					payment_identifier,
					e.to_string(),
				));
				return Err(e)
			},
		}

		match payment_completed.await {
			Ok(status) => match status {
				PaymentStatus::Success(target, identifier) => Ok(Payment {
					target,
					payment_identifier: identifier,
					secret,
					secrethash: secret_hash,
				}),
				PaymentStatus::Error(_target, _identifier, error) => Err(ApiError::State(error)),
			},
			Err(e) => Err(ApiError::State(format!("Could not receive payment status: {:?}", e))),
		}
	}

	pub async fn mint_token_for(
		&self,
		token_address: TokenAddress,
		to: Address,
		value: TokenAmount,
	) -> Result<TransactionHash, ApiError> {
		info!(
			message = "Mint token",
			token_address = token_address.checksum(),
			to = to.checksum(),
			value = value.to_string()
		);
		let token_proxy = self
			.raiden
			.proxy_manager
			.token(token_address)
			.await
			.map_err(ApiError::ContractSpec)?;

		let transaction_hash = token_proxy
			.mint_for(self.raiden.config.account.clone(), to, value)
			.await
			.map_err(ApiError::Proxy)?;

		Ok(transaction_hash)
	}

	fn check_invalid_channel_timeouts(
		&self,
		settle_timeout: SettleTimeout,
		reveal_timeout: RevealTimeout,
	) -> Result<(), ApiError> {
		if reveal_timeout < RevealTimeout::from(MIN_REVEAL_TIMEOUT) {
			if reveal_timeout <= RevealTimeout::from(0) {
				return Err(ApiError::Param("reveal_timeout should be larger than zero.".to_owned()))
			} else {
				return Err(ApiError::Param(format!(
					"reveal_timeout is lower than the required minimum value of {}",
					MIN_REVEAL_TIMEOUT,
				)))
			}
		}

		if settle_timeout < SettleTimeout::from(reveal_timeout * 2) {
			return Err(ApiError::Param(
				"`settle_timeout` can not be smaller than double the `reveal_timeout`.\n\n
                The setting `reveal_timeout` determines the maximum number of
                blocks it should take a transaction to be mined when the
                blockchain is under congestion. This setting determines the
                when a node must go on-chain to register a secret, and it is
                therefore the lower bound of the lock expiration. The
                `settle_timeout` determines when a channel can be settled
                on-chain, for this operation to be safe all locks must have
                been resolved, for this reason the `settle_timeout` has to be
                larger than `reveal_timeout`."
					.to_owned(),
			))
		}

		Ok(())
	}

	async fn initiator_init(
		&self,
		transfer_identifier: PaymentIdentifier,
		transfer_amount: TokenAmount,
		transfer_secret: Secret,
		transfer_secrethash: SecretHash,
		token_network_registry_address: TokenNetworkRegistryAddress,
		token_network_address: TokenNetworkAddress,
		target_address: Address,
		lock_timeout: Option<BlockTimeout>,
		route_states: Option<Vec<RouteState>>,
	) -> Result<ActionInitInitiator, ApiError> {
		let chain_state = self.raiden.state_manager.read().current_state.clone();
		let our_address = chain_state.our_address;
		let transfer_state = TransferDescriptionWithSecretState {
			token_network_registry_address,
			token_network_address,
			lock_timeout,
			payment_identifier: transfer_identifier,
			amount: transfer_amount,
			initiator: our_address,
			target: target_address,
			secret: transfer_secret,
			secrethash: transfer_secrethash,
		};

		let our_address_metadata = self.raiden.config.metadata.clone();
		let one_to_n_address = self.raiden.config.addresses.one_to_n;
		let from_address = self.raiden.config.account.address();

		let route_states = if let Some(route_states) = route_states {
			route_states
		} else {
			let (routes, _feedback_token) = routing::get_best_routes(
				self.raiden.pfs.clone(),
				chain_state,
				our_address_metadata,
				token_network_address,
				Some(one_to_n_address),
				from_address,
				target_address,
				transfer_amount,
				None,
			)
			.await
			.map_err(ApiError::Routing)?;

			routes
		};

		Ok(ActionInitInitiator { transfer: transfer_state, routes: route_states })
	}
}
