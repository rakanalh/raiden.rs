use std::cmp::max;

use crate::state_machine::state::{
    ChainState,
    TokenNetworkRegistryState,
    TokenNetworkState,
};
use web3::types::{
    Address,
    H256,
    U64,
};

use super::{
    state::{
        CanonicalIdentifier,
        ChannelEndState,
        ChannelState,
        TransactionResult,
    },
    types::ChannelStatus,
};

pub fn block_number(chain_state: &ChainState) -> U64 {
    chain_state.block_number
}

pub fn confirmed_block_hash(chain_state: &ChainState) -> H256 {
    chain_state.block_hash
}

pub fn get_token_network<'a>(
    chain_state: &'a ChainState,
    token_network_address: &'a Address,
) -> Option<&'a TokenNetworkState> {
    let mut token_network: Option<&TokenNetworkState> = None;

    let token_network_registries = &chain_state.identifiers_to_tokennetworkregistries;
    for token_network_registry in token_network_registries.values() {
        token_network = token_network_registry
            .tokennetworkaddresses_to_tokennetworks
            .get(token_network_address);
    }
    token_network.clone()
}

pub fn get_token_network_registry_by_token_network_address(
    chain_state: &ChainState,
    token_network_address: Address,
) -> Option<&TokenNetworkRegistryState> {
    let token_network_registries = &chain_state.identifiers_to_tokennetworkregistries;
    for token_network_registry in token_network_registries.values() {
        let token_network = token_network_registry
            .tokennetworkaddresses_to_tokennetworks
            .get(&token_network_address);
        if token_network.is_some() {
            return Some(token_network_registry);
        }
    }
    None
}

pub fn get_token_network_by_address(
    chain_state: &ChainState,
    token_network_address: Address,
) -> Option<&TokenNetworkState> {
    let token_network_registries = &chain_state.identifiers_to_tokennetworkregistries;
    token_network_registries
        .values()
        .map(|tnr| tnr.tokennetworkaddresses_to_tokennetworks.values())
        .flatten()
        .find(|tn| tn.address == token_network_address)
}

pub fn get_channels(chain_state: &ChainState) -> Vec<ChannelState> {
    let mut channels = vec![];

    for token_network_registry in chain_state.identifiers_to_tokennetworkregistries.values() {
        for token_network in token_network_registry.tokennetworkaddresses_to_tokennetworks.values() {
            channels.extend(token_network.channelidentifiers_to_channels.values().cloned());
        }
    }

    channels
}

pub fn get_channel_by_canonical_identifier(
    chain_state: &ChainState,
    canonical_identifier: CanonicalIdentifier,
) -> Option<&ChannelState> {
    let token_network = get_token_network_by_address(chain_state, canonical_identifier.token_network_address);
    if let Some(token_network) = token_network {
        return token_network
            .channelidentifiers_to_channels
            .get(&canonical_identifier.channel_identifier);
    }
    None
}

pub fn get_channel_status(channel_state: &ChannelState) -> ChannelStatus {
    let mut result = ChannelStatus::Opened;
    if let Some(settle_transaction) = &channel_state.settle_transaction {
        let finished_successfully = match &settle_transaction.result {
            Some(r) => *r == TransactionResult::Success,
            None => false,
        };
        let running = settle_transaction.finished_block_number.is_none();

        if finished_successfully {
            result = ChannelStatus::Settled;
        } else if running {
            result = ChannelStatus::Settling;
        } else {
            result = ChannelStatus::Unusable;
        }
    } else if let Some(close_transaction) = &channel_state.close_transaction {
        let finished_successfully = match &close_transaction.result {
            Some(r) => *r == TransactionResult::Success,
            None => false,
        };
        let running = close_transaction.finished_block_number.is_none();

        if finished_successfully {
            result = ChannelStatus::Closed;
        } else if running {
            result = ChannelStatus::Closing;
        } else {
            result = ChannelStatus::Unusable;
        }
    }

    result
}

pub fn get_channel_balance(sender: &ChannelEndState, receiver: &ChannelEndState) -> u64 {
    let mut sender_transferred_amount = 0;
    let mut receiver_transferred_amount = 0;

    if let Some(ref sender_balance_proof) = sender.balance_proof {
        sender_transferred_amount = sender_balance_proof.transferred_amount;
    }

    if let Some(ref receiver_balance_proof) = receiver.balance_proof {
        receiver_transferred_amount = receiver_balance_proof.transferred_amount;
    }

    sender.contract_balance
        - max(sender.offchain_total_withdraw(), sender.onchain_total_withdraw)
        - sender_transferred_amount
        + receiver_transferred_amount
}
