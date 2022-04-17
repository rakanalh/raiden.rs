use std::sync::Arc;

use ethabi::Token;
use web3::types::{
    Address,
    Bytes,
};

use super::{
    events::Event,
    proxies::ProxyManager,
};
use crate::{
    constants,
    primitives::{
        BlockHash,
        BlockNumber,
        CanonicalIdentifier,
        RaidenConfig,
        RevealTimeout,
        SettleTimeout,
        TransactionExecutionStatus,
        TransactionResult,
    },
    state_machine::{
        types::{
            ChainState,
            ChannelState,
            ContractReceiveChannelClosed,
            ContractReceiveChannelDeposit,
            ContractReceiveChannelOpened,
            ContractReceiveChannelSettled,
            ContractReceiveChannelWithdraw,
            ContractReceiveTokenNetworkCreated,
            ContractReceiveUpdateTransfer,
            StateChange,
            TokenNetworkState,
            TransactionChannelDeposit,
        },
        views,
    },
    storage::Storage,
};
use derive_more::Display;
use thiserror::Error;

#[derive(Error, Debug, Display)]
pub struct DecodeError(String);

pub type Result<T> = std::result::Result<T, DecodeError>;

pub struct EventDecoder {
    proxy_manager: Arc<ProxyManager>,
    config: RaidenConfig,
}

impl EventDecoder {
    pub fn new(config: RaidenConfig, proxy_manager: Arc<ProxyManager>) -> Self {
        Self { proxy_manager, config }
    }

    pub async fn as_state_change(
        &self,
        event: Event,
        chain_state: &ChainState,
        _storage: Arc<Storage>,
    ) -> Result<Option<StateChange>> {
        match event.name.as_ref() {
            "TokenNetworkCreated" => self.token_network_created(event),
            "ChannelOpened" => self.channel_opened(chain_state, event),
            "ChannelNewDeposit" => self.channel_deposit(chain_state, event),
            "ChannelWithdraw" => self.channel_withdraw(chain_state, event),
            "ChannelClosed" => self.channel_closed(chain_state, event),
            "ChannelSettled" => self.channel_settled(chain_state, event).await,
            // "ChannelUnlocked" => self.channel_unlocked(chain_state, event, storage).await,
            "NonClosingBalanceProofUpdated" => self.channel_non_closing_balance_proof_updated(chain_state, event),
            _ => Err(DecodeError(format!("Event {} unknown", event.name))),
        }
    }

    fn token_network_created(&self, event: Event) -> Result<Option<StateChange>> {
        let token_address = match event.data.get("token_address") {
            Some(Token::Address(address)) => address.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid token address",
                    event.name,
                )))
            }
        };
        let token_network_address = match event.data.get("token_network_address") {
            Some(Token::Address(address)) => address.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid token network address",
                    event.name,
                )))
            }
        };
        let token_network = TokenNetworkState::new(token_network_address, token_address);
        let token_network_registry_address = event.address;
        Ok(Some(
            ContractReceiveTokenNetworkCreated {
                transaction_hash: Some(event.transaction_hash),
                block_number: event.block_number,
                block_hash: event.block_hash,
                token_network_registry_address,
                token_network,
            }
            .into(),
        ))
    }

    fn channel_opened(&self, chain_state: &ChainState, event: Event) -> Result<Option<StateChange>> {
        let channel_identifier = match event.data.get("channel_identifier") {
            Some(Token::Uint(identifier)) => identifier.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid channel identifier",
                    event.name,
                )))
            }
        };
        let participant1 = match event.data.get("participant1") {
            Some(Token::Address(address)) => address.clone(),
            _ => return Err(DecodeError(format!("{} event has an invalid participant1", event.name))),
        };
        let participant2 = match event.data.get("participant2") {
            Some(Token::Address(address)) => address.clone(),
            _ => return Err(DecodeError(format!("{} event has an invalid participant2", event.name))),
        };
        let settle_timeout = match event.data.get("settle_timeout") {
            Some(Token::Uint(timeout)) => SettleTimeout::from(timeout.clone().low_u64()),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid settle timeout",
                    event.name,
                )))
            }
        };

        let partner_address: Address;
        let our_address = chain_state.our_address;
        if our_address == participant1 {
            partner_address = participant2;
        } else if our_address == participant2 {
            partner_address = participant1;
        } else {
            return Ok(None);
        }

        let token_network_address = event.address;
        let token_network = views::get_token_network_by_address(&chain_state, token_network_address)
            .ok_or_else(|| DecodeError(format!("{} event haswan unknown Token network address", event.name)))?;
        let token_address = token_network.token_address;
        let token_network_registry_address = Address::zero();
        let reveal_timeout = RevealTimeout::from(constants::DEFAULT_REVEAL_TIMEOUT);
        let open_transaction = TransactionExecutionStatus {
            started_block_number: Some(BlockNumber::from(0)),
            finished_block_number: Some(event.block_number.clone()),
            result: Some(TransactionResult::Success),
        };
        let channel_state = ChannelState::new(
            CanonicalIdentifier {
                chain_identifier: chain_state.chain_id.clone(),
                token_network_address,
                channel_identifier,
            },
            token_address,
            token_network_registry_address,
            our_address,
            partner_address,
            reveal_timeout,
            settle_timeout,
            open_transaction,
            self.config.mediation_config.clone(),
        )
        .map_err(|e| DecodeError(format!("{:?}", e)))?;

        Ok(Some(
            ContractReceiveChannelOpened {
                transaction_hash: Some(event.transaction_hash),
                block_number: event.block_number,
                block_hash: event.block_hash,
                channel_state,
            }
            .into(),
        ))
    }

    fn channel_deposit(&self, chain_state: &ChainState, event: Event) -> Result<Option<StateChange>> {
        let token_network_address = event.address;
        let channel_identifier = match event.data.get("channel_identifier") {
            Some(Token::Uint(identifier)) => identifier.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid channel identifier",
                    event.name,
                )))
            }
        };
        let participant = match event.data.get("participant") {
            Some(Token::Address(address)) => address.clone(),
            _ => return Err(DecodeError(format!("{} event has an invalid participant", event.name))),
        };
        let total_deposit = match event.data.get("total_deposit") {
            Some(Token::Uint(total_deposit)) => total_deposit.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid total deposit",
                    event.name,
                )))
            }
        };
        let channel_deposit = ContractReceiveChannelDeposit {
            canonical_identifier: CanonicalIdentifier {
                chain_identifier: chain_state.chain_id.clone(),
                token_network_address,
                channel_identifier,
            },
            deposit_transaction: TransactionChannelDeposit {
                participant_address: participant,
                contract_balance: total_deposit,
                deposit_block_number: event.block_number,
            },
            fee_config: self.config.mediation_config.clone(),
            transaction_hash: Some(event.transaction_hash),
            block_number: event.block_number,
            block_hash: event.block_hash,
        };
        Ok(Some(channel_deposit.into()))
    }

    fn channel_withdraw(&self, chain_state: &ChainState, event: Event) -> Result<Option<StateChange>> {
        let token_network_address = event.address;
        let channel_identifier = match event.data.get("channel_identifier") {
            Some(Token::Uint(identifier)) => identifier.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid channel identifier",
                    event.name,
                )))
            }
        };
        let participant = match event.data.get("participant") {
            Some(Token::Address(address)) => address.clone(),
            _ => return Err(DecodeError(format!("{} event has an invalid participant", event.name,))),
        };
        let total_withdraw = match event.data.get("total_withdraw") {
            Some(Token::Uint(total_withdraw)) => total_withdraw.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid total withdraw",
                    event.name,
                )))
            }
        };
        let channel_withdraw = ContractReceiveChannelWithdraw {
            canonical_identifier: CanonicalIdentifier {
                chain_identifier: chain_state.chain_id.clone(),
                token_network_address,
                channel_identifier,
            },
            participant,
            total_withdraw,
            fee_config: self.config.mediation_config.clone(),
            transaction_hash: Some(event.transaction_hash),
            block_number: event.block_number,
            block_hash: event.block_hash,
        };
        Ok(Some(channel_withdraw.into()))
    }

    fn channel_closed(&self, chain_state: &ChainState, event: Event) -> Result<Option<StateChange>> {
        let channel_identifier = match event.data.get("channel_identifier") {
            Some(Token::Uint(identifier)) => identifier.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid channel identifier",
                    event.name,
                )))
            }
        };
        let transaction_from = match event.data.get("closing_participant") {
            Some(Token::Address(address)) => address.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid closing participant",
                    event.name,
                )))
            }
        };
        let token_network_address = event.address;
        let channel_closed = ContractReceiveChannelClosed {
            transaction_hash: Some(event.transaction_hash),
            block_number: event.block_number,
            block_hash: event.block_hash,
            transaction_from,
            canonical_identifier: CanonicalIdentifier {
                chain_identifier: chain_state.chain_id.clone(),
                token_network_address,
                channel_identifier,
            },
        };
        Ok(Some(channel_closed.into()))
    }

    fn channel_non_closing_balance_proof_updated(
        &self,
        chain_state: &ChainState,
        event: Event,
    ) -> Result<Option<StateChange>> {
        let token_network_address = event.address;
        let channel_identifier = match event.data.get("channel_identifier") {
            Some(Token::Uint(identifier)) => identifier.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event has an invalid channel_identifier",
                    event.name,
                )))
            }
        };
        let nonce = match event.data.get("nonce") {
            Some(Token::Uint(nonce)) => nonce.clone(),
            _ => return Err(DecodeError(format!("{} event has an invalid nonce", event.name,))),
        };
        let update_transfer = ContractReceiveUpdateTransfer {
            canonical_identifier: CanonicalIdentifier {
                chain_identifier: chain_state.chain_id.clone(),
                token_network_address,
                channel_identifier,
            },
            nonce,
            transaction_hash: Some(event.transaction_hash),
            block_number: event.block_number,
            block_hash: event.block_hash,
        };
        Ok(Some(update_transfer.into()))
    }

    async fn channel_settled(&self, chain_state: &ChainState, event: Event) -> Result<Option<StateChange>> {
        let token_network_address = event.address;
        let channel_identifier = match event.data.get("channel_identifier") {
            Some(Token::Uint(identifier)) => identifier.clone(),
            _ => {
                return Err(DecodeError(format!(
                    "{} event arg `channel_identifier` invalid",
                    event.name,
                )))
            }
        };

        let channel_state = match views::get_channel_by_canonical_identifier(
            chain_state,
            CanonicalIdentifier {
                chain_identifier: chain_state.chain_id.clone(),
                token_network_address,
                channel_identifier,
            },
        ) {
            Some(channel_state) => channel_state,
            None => return Ok(None),
        };

        let (our_onchain_locksroot, partner_onchain_locksroot) = self
            .get_onchain_locksroot(channel_state, chain_state.block_hash)
            .await?;

        let channel_settled = ContractReceiveChannelSettled {
            transaction_hash: Some(event.transaction_hash),
            block_number: event.block_number,
            block_hash: event.block_hash,
            canonical_identifier: CanonicalIdentifier {
                chain_identifier: chain_state.chain_id.clone(),
                token_network_address,
                channel_identifier,
            },
            our_onchain_locksroot,
            partner_onchain_locksroot,
        };
        Ok(Some(channel_settled.into()))
    }

    // async fn channel_unlocked(&self, chain_state: &ChainState, event: Event, storage: Arc<Storage>) -> Result<Option<StateChange>> {
    //     let token_network_address = event.address;
    //     let participant1 = match event.data.get("sender") {
    //         Some(Token::Address(address)) => address.clone(),
    //         _ => return Err(DecodeError(format!("{} event has an invalid sender", event.name))),
    //     };
    //     let participant2 = match event.data.get("receiver") {
    //         Some(Token::Address(address)) => address.clone(),
    //         _ => return Err(DecodeError(format!("{} event has an invalid receiver", event.name))),
    //     };
    //     let locksroot = match event.data.get("locksroot") {
    //         Some(Token::Bytes(bytes)) => bytes.clone(),
    //         _ => return Err(DecodeError(format!("{} event has an invalid locksroot", event.name))),
    //     };
    //     let unlocked_amount = match event.data.get("unlocked_amount") {
    //         Some(Token::Uint(amount)) => amount.clone(),
    //         _ => return Err(DecodeError(format!("{} event has an invalid unlocked amount", event.name))),
    //     };
    //     let returned_tokens = match event.data.get("returned_tokens") {
    //         Some(Token::Uint(amount)) => amount.clone(),
    //         _ => return Err(DecodeError(format!("{} event has an invalid returned tokens", event.name))),
    //     };
    //     let token_network = match views::get_token_network_by_address(chain_state, token_network_address) {
    //         Some(token_network) => token_network,
    //         None => return Ok(None),
    //     };

    //     let partner = if participant1 == chain_state.our_address {
    //         participant2
    //     } else if participant2 == chain_state.our_address {
    //         participant1
    //     } else {
    //         return Ok(None);
    //     };

    //     let channel_identifiers = token_network.channelidentifiers_to_channels.keys();
    //     let canonical_identifier = for channel_identifier in channel_identifiers {
    //          if partner == participant1 {
    //             let criteria = vec![
    //                 ("balance_proof.canonical_identifier.chain_identifier".to_owned(), format!("{}", chain_state.chain_id)),
    //                 ("balance_proof.canonical_identifier.token_network_address".to_owned(), format!("{}", token_network_address)),
    //                 ("balance_proof.canonical_identifier.channel_identifier".to_owned(), format!("{}", channel_identifier)),
    //                 ("balance_proof.locksroot".to_owned(), format!("{:?}", locksroot)),
    //                 ("balance_proof.sender".to_owned(), format!("{}", participant1))
    //             ];
    //             let state_change_record = match storage.get_latest_state_change_by_data_field(criteria) {
    //                 Ok(Some(state_change_record)) => state_change_record,
    //                 _ => continue,
    //             };

    //             let state_change: StateChange = match serde_json::from_str(&state_change_record.data) {
    //                 Ok(state_change) => state_change,
    //                 _ => return Err(DecodeError(format!("{}: Could not restore state change", event.name))),
    //             };
    //         } else if partner == participant2 {

    //         } else {
    //             return Ok(None);
    //         };

    //     };

    //     let channel_unlocked = ContractReceiveChannelBatchUnlock {
    //         canonical_identifier,
    //         receiver: participant2,
    //         sender: participant1,
    //         locksroot,
    //         unlocked_amount,
    //         returned_tokens,
    //     };
    //     Ok(Some(StateChange::ContractReceiveChannelBatchUnlock(channel_unlocked)))
    // }

    async fn get_onchain_locksroot(&self, channel_state: &ChannelState, block: BlockHash) -> Result<(Bytes, Bytes)> {
        let payment_channel = self
            .proxy_manager
            .payment_channel(&channel_state)
            .await
            .map_err(|e| DecodeError(format!("{:?}", e)))?;
        let (our_data, partner_data) = payment_channel
            .token_network
            .participants_details(
                channel_state.canonical_identifier.channel_identifier,
                channel_state.our_state.address,
                channel_state.partner_state.address,
                block,
            )
            .await
            .map_err(|e| DecodeError(format!("{:?}", e)))?;

        Ok((our_data.locksroot, partner_data.locksroot))
    }

    #[allow(dead_code)]
    fn get_state_change_with_balance_proof_by_locksroot() {}

    #[allow(dead_code)]
    fn get_event_with_balance_proof_by_locks_root() {}
}
