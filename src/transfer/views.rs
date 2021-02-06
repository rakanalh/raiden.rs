use crate::transfer::state::{ChainState, TokenNetworkRegistryState, TokenNetworkState};
use web3::types::{Address, U64};

pub fn block_number(chain_state: &ChainState) -> U64 {
	chain_state.block_number
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
