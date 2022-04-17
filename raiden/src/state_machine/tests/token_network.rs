use std::collections::HashMap;

use ethabi::{
    ethereum_types::H256,
    Address,
};

use crate::{
    primitives::U64,
    state_machine::{
        machine::chain,
        types::{
            ContractReceiveTokenNetworkCreated,
            StateChange,
            TokenNetworkGraphState,
            TokenNetworkState,
        },
        views,
    },
    tests::factories::chain_state_with_token_network_registry,
};

#[test]
fn create_token_network() {
    let token_network_registry_address = Address::random();
    let token_network_address = Address::random();
    let token_address = Address::random();
    let chain_state = chain_state_with_token_network_registry(token_network_registry_address);

    let state_change = ContractReceiveTokenNetworkCreated {
        transaction_hash: Some(H256::random()),
        token_network_registry_address,
        token_network: TokenNetworkState {
            address: token_network_address,
            token_address,
            network_graph: TokenNetworkGraphState {},
            channelidentifiers_to_channels: HashMap::new(),
            partneraddresses_to_channelidentifiers: HashMap::new(),
        },
        block_number: U64::from(1u64),
        block_hash: H256::random(),
    };
    let result = chain::state_transition(chain_state, state_change.into()).expect("State transition should succeed");

    let token_network = views::get_token_network_by_address(&result.new_state, token_network_address);
    assert!(token_network.is_some());
    let token_network = token_network.unwrap();
    assert_eq!(token_network.address, token_network_address);
    assert_eq!(token_network.token_address, token_address);

    let token_network =
        views::get_token_network_by_token_address(&result.new_state, token_network_registry_address, token_address);
    assert!(token_network.is_some());
    let token_network = token_network.unwrap();
    assert_eq!(token_network.address, token_network_address);
    assert_eq!(token_network.token_address, token_address);
}
