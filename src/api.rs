use parking_lot::RwLock;
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
    state_machine::views,
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
    #[error("Invalid parameter: `{0}`")]
    Param(String),
}

pub struct Api {
    state_manager: Arc<RwLock<StateManager>>,
    proxy_manager: Arc<ProxyManager>,
}

impl Api {
    pub fn new(state_manager: Arc<RwLock<StateManager>>, proxy_manager: Arc<ProxyManager>) -> Self {
        Self {
            state_manager,
            proxy_manager,
        }
    }

    fn check_invalid_channel_timeouts(&self, settle_timeout: U256, reveal_timeout: U256) -> Result<(), ApiError> {
        if reveal_timeout < U256::from(constants::MIN_REVEAL_TIMEOUT) {
            if reveal_timeout <= U256::from(0) {
                return Err(ApiError::Param("reveal_timeout should be larger than zero.".to_owned()));
            } else {
                return Err(ApiError::Param(format!(
                    "reveal_timeout is lower than the required minimum value of {}",
                    constants::MIN_REVEAL_TIMEOUT,
                )));
            }
        }

        if settle_timeout < reveal_timeout * 2 {
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

    pub async fn create_channel(
        &self,
        registry_address: Address,
        token_address: Address,
        partner_address: Address,
        settle_timeout: Option<U256>,
        reveal_timeout: Option<U256>,
        total_deposit: Option<U256>,
        retry_timeout: Option<f32>,
    ) -> Result<U256, ApiError> {
        let current_state = &self.state_manager.read().current_state.clone();
        let our_address = current_state.our_address;

        let token_proxy = self
            .proxy_manager
            .token(token_address, our_address)
            .await
            .map_err(ApiError::ContractSpec)?;

        let balance = token_proxy
            .balance_of(our_address, views::confirmed_block_hash(&current_state))
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
        let reveal_timeout = reveal_timeout.unwrap_or(U256::from(constants::DEFAULT_REVEAL_TIMEOUT));
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
            .token_network(token_address, token_network_address, our_address)
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
}
