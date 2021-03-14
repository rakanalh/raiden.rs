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

use super::state::ChannelState;

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

pub fn get_channels(chain_state: &ChainState) -> Vec<ChannelState> {
    let mut channels = vec![];

    for token_network_registry in chain_state.identifiers_to_tokennetworkregistries.values() {
        for token_network in token_network_registry.tokennetworkaddresses_to_tokennetworks.values() {
            channels.extend(token_network.channelidentifiers_to_channels.values().cloned());
        }
    }

	channels
}
