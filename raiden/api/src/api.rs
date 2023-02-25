use std::sync::Arc;

use raiden_blockchain::{
	errors::ContractDefError,
	proxies::{
		Account,
		GasReserve,
		ProxyError,
	},
};
use raiden_pathfinding::{
	routing,
	RoutingError,
};
use raiden_primitives::types::{
	Address,
	BlockTimeout,
	Bytes,
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
};
use raiden_state_machine::{
	constants::{
		ABSENT_SECRET,
		DEFAULT_REVEAL_TIMEOUT,
		DEFAULT_SETTLE_TIMEOUT,
		MIN_REVEAL_TIMEOUT,
		SECRET_LENGTH,
	},
	errors::StateTransitionError,
	types::{
		ActionChannelSetRevealTimeout,
		ActionInitInitiator,
		ChannelState,
		ChannelStatus,
		RouteState,
		TransferDescriptionWithSecretState,
	},
	views,
};
use raiden_transition::Transitioner;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::info;
use web3::{
	signing::keccak256,
	transports::Http,
};

use crate::{
	payments::PaymentsRegistry,
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

pub struct Api {
	raiden: Arc<Raiden>,
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
            "Opening channel. registry_address={}, partner_address={}, token_address={}, settle_timeout={:?}, reveal_timeout={:?}.",
            registry_address,
            partner_address,
            token_address,
            settle_timeout,
            reveal_timeout,
        );
		let settle_timeout = settle_timeout.unwrap_or(SettleTimeout::from(DEFAULT_SETTLE_TIMEOUT));
		let reveal_timeout = reveal_timeout.unwrap_or(RevealTimeout::from(DEFAULT_REVEAL_TIMEOUT));

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
				confirmed_block_identifier,
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

		let channel_details = token_network
			.new_channel(
				account.clone(),
				partner_address,
				settle_timeout,
				confirmed_block_identifier,
			)
			.await
			.map_err(ApiError::Proxy)?;

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
		self.transition_service
			.transition(
				ActionChannelSetRevealTimeout {
					canonical_identifier: channel_state.canonical_identifier.clone(),
					reveal_timeout,
				}
				.into(),
			)
			.await;

		Ok(channel_details)
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
            "Patching channel. registry_address={}, partner_Address={}, token_address={}, reveal_timeout={:?}, total_deposit={:?}, total_withdraw={:?}, state={:?}.",
            registry_address,
            partner_address,
            token_address,
            reveal_timeout,
            total_deposit,
            total_withdraw,
            state,
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
				return self.channel_close(channel_state).await
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
			"Depositing to channel. channel_identifier={}, total_deposit={:?}.",
			channel_state.canonical_identifier.channel_identifier, total_deposit,
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

		let deposit_increase = total_deposit - channel_state.our_state.contract_balance;

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
		_channel_state: &ChannelState,
		_total_withdraw: TokenAmount,
	) -> Result<(), ApiError> {
		return Err(ApiError::State(format!("Not implemented")))
	}

	pub async fn channel_reveal_timeout(
		&self,
		_channel_state: &ChannelState,
		_reveal_timeout: RevealTimeout,
	) -> Result<(), ApiError> {
		return Err(ApiError::State(format!("Not implemented")))
	}

	pub async fn channel_close(&self, _channel_state: &ChannelState) -> Result<(), ApiError> {
		return Err(ApiError::State(format!("Not implemented")))
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
	) -> Result<(), ApiError> {
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
			None => keccak256(&secret.0).into(),
		};

		if !secret.0.is_empty() {
			if secret_hash != keccak256(&secret.0).into() {
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
				secret,
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
				self.transition_service.transition(action_init_initiator.into()).await;
			},
			Err(e) => {
				self.payments_registry
					.write()
					.await
					.complete(partner_address, payment_identifier);
				return Err(e)
			},
		}

		let _ = payment_completed.await;

		Ok(())
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
				self.raiden.config.pfs_config.clone(),
				self.raiden.config.account.private_key(),
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
