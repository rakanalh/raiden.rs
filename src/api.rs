use parking_lot::RwLock;
use slog::{
    info,
    Logger,
};
use std::sync::Arc;
use thiserror::Error;
use web3::types::{
    Address,
    U256,
};

use crate::{
    blockchain::{
        errors::ContractDefError,
        proxies::{
            ProxyError,
            ProxyManager,
        },
    },
    constants,
    primitives::U64,
    state_machine::{
        types::{
            ChannelState,
            ChannelStatus,
        },
        views,
    },
    state_manager::StateManager,
};

#[derive(Error, Debug)]
pub enum ContractError {}

#[derive(Error, Debug)]
pub enum ApiError {
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
    #[error("Invalid parameter: `{0}`")]
    Param(String),
}

pub struct Api {
    state_manager: Arc<RwLock<StateManager>>,
    proxy_manager: Arc<ProxyManager>,
    logger: Logger,
}

impl Api {
    pub fn new(state_manager: Arc<RwLock<StateManager>>, proxy_manager: Arc<ProxyManager>, logger: Logger) -> Self {
        Self {
            state_manager,
            proxy_manager,
            logger,
        }
    }

    pub async fn create_channel(
        &self,
        registry_address: Address,
        token_address: Address,
        partner_address: Address,
        settle_timeout: Option<U256>,
        reveal_timeout: Option<U64>,
        total_deposit: Option<U256>,
        retry_timeout: Option<f32>,
    ) -> Result<U256, ApiError> {
        let current_state = &self.state_manager.read().current_state.clone();

        info!(
            self.logger,
            "Opening channel. registry_address={}, partner_address={}, token_address={}, settle_timeout={:?}, reveal_timeout={:?}.",
            registry_address,
            partner_address,
            token_address,
            settle_timeout,
            reveal_timeout,
        );
        let our_address = current_state.our_address;

        let token_proxy = self
            .proxy_manager
            .token(token_address)
            .await
            .map_err(ApiError::ContractSpec)?;

        let balance = token_proxy
            .balance_of(our_address, Some(views::confirmed_block_hash(&current_state)))
            .await
            .map_err(ApiError::Proxy)?;

        match total_deposit {
            Some(total_deposit) => {
                if total_deposit > balance {
                    return Err(ApiError::Param(format!(
                        "Not enough balance to deposit. {} Available={} Needed {}",
                        token_address, balance, total_deposit
                    )));
                }
            }
            _ => {}
        };

        let settle_timeout = settle_timeout.unwrap_or(U256::from(constants::DEFAULT_SETTLE_TIMEOUT));
        let reveal_timeout = reveal_timeout.unwrap_or(U64::from(constants::DEFAULT_REVEAL_TIMEOUT));
        let _retry_timeout = retry_timeout.unwrap_or(constants::DEFAULT_RETRY_TIMEOUT);

        self.check_invalid_channel_timeouts(settle_timeout, reveal_timeout)?;

        let confirmed_block_identifier = views::confirmed_block_hash(&current_state);
        let registry = self
            .proxy_manager
            .token_network_registry(registry_address, our_address)
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
            )));
        }

        if settle_timeout > settlement_timeout_max {
            return Err(ApiError::Param(format!(
                "Settlement timeout exceeds max of {}",
                settlement_timeout_max,
            )));
        }

        let token_network_address = registry
            .get_token_network(token_address, confirmed_block_identifier)
            .await
            .map_err(ApiError::Proxy)?;

        if token_network_address.is_zero() {
            return Err(ApiError::Param(format!(
                "Token network for token {} does not exist",
                token_address,
            )));
        }

        let token_network = self
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
            ));
        }

        let duplicated_channel = token_network
            .get_channel_identifier(token_network_address, partner_address, confirmed_block_identifier)
            .await
            .map_err(ApiError::Proxy)?;

        if duplicated_channel.is_some() {
            return Err(ApiError::OnChainCall(format!(
                "A channel with {} for token
                {} already exists.
                (At blockhash: {})",
                partner_address, token_address, confirmed_block_identifier,
            )));
        }

        // has_enough_reserve, estimated_required_reserve = has_enough_gas_reserve(
        //     self.raiden, channels_to_open=1
        // )

        // if not has_enough_reserve:
        //     raise InsufficientGasReserve(
        //         "The account balance is below the estimated amount necessary to "
        //         "finish the lifecycles of all active channels. A balance of at "
        //         f"least {estimated_required_reserve} wei is required."
        //     )

        let channel_details = token_network
            .new_channel(partner_address, settle_timeout, confirmed_block_identifier)
            .await
            .map_err(ApiError::Proxy)?;

        Ok(channel_details)
        // except DuplicatedChannelError:
        //     log.info("partner opened channel first")
        // except RaidenRecoverableError:
        //     # The channel may have been created in the pending block.
        //     duplicated_channel = self.is_already_existing_channel(
        //         token_network_address=token_network_address, partner_address=partner_address
        //     )
        //     if duplicated_channel:
        //         log.info("Channel has already been opened")
        //     else:
        //         raise

        // waiting.wait_for_newchannel(
        //     raiden=self.raiden,
        //     token_network_registry_address=registry_address,
        //     token_address=token_address,
        //     partner_address=partner_address,
        //     retry_timeout=retry_timeout,
        // )

        // chain_state = views.state_from_raiden(self.raiden)
        // channel_state = views.get_channelstate_for(
        //     chain_state=chain_state,
        //     token_network_registry_address=registry_address,
        //     token_address=token_address,
        //     partner_address=partner_address,
        // )

        // assert channel_state, f"channel {channel_state} is gone"

        // self.raiden.set_channel_reveal_timeout(
        //     canonical_identifier=channel_state.canonical_identifier, reveal_timeout=reveal_timeout
        // )

        // return channel_state.identifier
    }

    pub async fn update_channel(
        &self,
        registry_address: Address,
        token_address: Address,
        partner_address: Address,
        reveal_timeout: Option<U64>,
        total_deposit: Option<U256>,
        total_withdraw: Option<U256>,
        state: Option<ChannelStatus>,
    ) -> Result<(), ApiError> {
        info!(
            self.logger,
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
            )));
        }

        if total_deposit.is_some() && state.is_some() {
            return Err(ApiError::Param(format!(
                "Can not update a channel's total deposit and state at the same time",
            )));
        }

        if total_withdraw.is_some() && state.is_some() {
            return Err(ApiError::Param(format!(
                "Can not update a channel's total withdraw and state at the same time",
            )));
        }

        if total_withdraw.is_some() && total_deposit.is_some() {
            return Err(ApiError::Param(format!(
                "Can not update a channel's total withdraw and total deposit at the same time",
            )));
        }

        if reveal_timeout.is_some() && total_deposit.is_some() {
            return Err(ApiError::Param(format!(
                "Can not update a channel's reveal timeout and total deposit at the same time",
            )));
        }

        if reveal_timeout.is_some() && total_withdraw.is_some() {
            return Err(ApiError::Param(format!(
                "Can not update a channel's reveal timeout and total withdraw at the same time",
            )));
        }

        if let Some(total_deposit) = total_deposit {
            if total_deposit < U256::zero() {
                return Err(ApiError::Param(format!("Amount to deposit must not be negative")));
            }
        }

        if let Some(total_withdraw) = total_withdraw {
            if total_withdraw < U256::zero() {
                return Err(ApiError::Param(format!("Amount to withdraw must not be negative")));
            }
        }

        let empty_request =
            total_deposit.is_none() && state.is_none() && total_withdraw.is_none() && reveal_timeout.is_none();

        if empty_request {
            return Err(ApiError::Param(format!(
                "Nothing to do. Should either provide \
                `total_deposit, `total_withdraw`, `reveal_timeout` or `state` argument"
            )));
        }

        let current_state = &self.state_manager.read().current_state.clone();
        let channel_state =
            match views::get_channel_state_for(current_state, registry_address, token_address, partner_address) {
                Some(channel_state) => channel_state,
                None => {
                    return Err(ApiError::State(format!(
                        "Requested channel for token {} and partner {} not found",
                        token_address, partner_address,
                    )));
                }
            };

        let result = if let Some(total_deposit) = total_deposit {
            self.channel_deposit(channel_state, total_deposit).await
        } else if let Some(total_withdraw) = total_withdraw {
            self.channel_withdraw(channel_state, total_withdraw).await
        } else if let Some(reveal_timeout) = reveal_timeout {
            self.channel_reveal_timeout(channel_state, reveal_timeout).await
        } else if let Some(state) = state {
            if state == ChannelStatus::Closed {
                return self.channel_close(channel_state).await;
            }
            return Err(ApiError::Param(format!("Unreachable")));
        } else {
            return Err(ApiError::Param(format!("Unreachable")));
        };

        result
    }

    pub async fn channel_deposit(
        &self,
        channel_state: &ChannelState,
        total_deposit: U256,
    ) -> Result<(), ApiError> {
        info!(
            self.logger,
            "Depositing to channel. channel_identifier={}, total_deposit={:?}.",
            channel_state.canonical_identifier.channel_identifier,
            total_deposit,
        );

        if views::get_channel_status(channel_state) != ChannelStatus::Opened {
            return Err(ApiError::State(format!("Can't set total deposit on a closed channel")));
        }

        let chain_state = &self.state_manager.read().current_state.clone();
        let confirmed_block_identifier = views::confirmed_block_hash(chain_state);
        let token = self
            .proxy_manager
            .token(channel_state.token_address)
            .await
            .map_err(ApiError::ContractSpec)?;

        let token_network_registry = self
            .proxy_manager
            .token_network_registry(channel_state.token_network_registry_address, chain_state.our_address)
            .await
            .map_err(ApiError::ContractSpec)?;

        let token_network_address = token_network_registry
            .get_token_network(channel_state.token_address, confirmed_block_identifier)
            .await
            .map_err(ApiError::Proxy)?;

        let token_network_proxy = self
            .proxy_manager
            .token_network(
                channel_state.token_address,
                token_network_address,
            )
            .await
            .map_err(ApiError::ContractSpec)?;

        let channel_proxy = self
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
            )));
        }

        if total_deposit <= channel_state.our_state.contract_balance {
            return Err(ApiError::State(format!("Total deposit did not increase.")));
        }

        // If this check succeeds it does not imply the `deposit` will
        // succeed, since the `deposit` transaction may race with another
        // transaction.
        if balance < deposit_increase {
            return Err(ApiError::State(format!(
                "Not enough balance to deposit. Available={} Needed={}",
                balance, deposit_increase,
            )));
        }

        if network_balance + deposit_increase > token_network_deposit_limit {
            return Err(ApiError::State(format!(
                "Deposit of {} would have exceeded \
                the token network deposit limit.",
                deposit_increase,
            )));
        }

        if total_deposit > channel_participant_deposit_limit {
            return Err(ApiError::State(format!(
                "Deposit of {} is larger than the \
                channel participant deposit limit",
                total_deposit,
            )));
        }

        if total_channel_deposit_overflow {
            return Err(ApiError::State(format!("Deposit overflow",)));
        }

        let _ = channel_proxy
            .approve_and_set_total_deposit(
                channel_state.canonical_identifier.channel_identifier,
                channel_state.partner_state.address,
                total_deposit,
                blockhash,
            )
            .await;

        Ok(())
    }

    pub async fn channel_withdraw(
        &self,
        _channel_state: &ChannelState,
        _total_withdraw: U256,
    ) -> Result<(), ApiError> {
        return Err(ApiError::State(format!("Not implemented")));
    }

    pub async fn channel_reveal_timeout(
        &self,
        _channel_state: &ChannelState,
        _reveal_timeout: U64,
    ) -> Result<(), ApiError> {
        return Err(ApiError::State(format!("Not implemented")));
    }

    pub async fn channel_close(&self, _channel_state: &ChannelState) -> Result<(), ApiError> {
        return Err(ApiError::State(format!("Not implemented")));
    }

    fn check_invalid_channel_timeouts(&self, settle_timeout: U256, reveal_timeout: U64) -> Result<(), ApiError> {
        if reveal_timeout < U64::from(constants::MIN_REVEAL_TIMEOUT) {
            if reveal_timeout <= U64::from(0) {
                return Err(ApiError::Param("reveal_timeout should be larger than zero.".to_owned()));
            } else {
                return Err(ApiError::Param(format!(
                    "reveal_timeout is lower than the required minimum value of {}",
                    constants::MIN_REVEAL_TIMEOUT,
                )));
            }
        }

        if settle_timeout < U256::from(reveal_timeout * 2) {
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
            ));
        }

        Ok(())
    }
}
