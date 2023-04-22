use std::collections::HashMap;

use raiden_primitives::types::{
	Address,
	BlockHash,
	BlockNumber,
};

use crate::{
	machine::chain,
	tests::factories::ChainStateBuilder,
	types::{
		ContractReceiveTokenNetworkCreated,
		TokenNetworkGraphState,
		TokenNetworkState,
	},
	views,
};

#[test]
fn create_token_network() {
	let chain_info = ChainStateBuilder::new().with_token_network_registry().build();

	let token_network_address = Address::random();
	let state_change = ContractReceiveTokenNetworkCreated {
		transaction_hash: Some(BlockHash::random()),
		token_network_registry_address: chain_info.token_network_registry_address,
		token_network: TokenNetworkState {
			address: token_network_address,
			token_address: chain_info.token_address,
			network_graph: TokenNetworkGraphState {},
			channelidentifiers_to_channels: HashMap::new(),
			partneraddresses_to_channelidentifiers: HashMap::new(),
		},
		block_number: BlockNumber::from(1u64),
		block_hash: BlockHash::random(),
	};
	let result = chain::state_transition(chain_info.chain_state, state_change.into())
		.expect("State transition should succeed");

	let token_network =
		views::get_token_network_by_address(&result.new_state, token_network_address);
	assert!(token_network.is_some());

	let token_network = token_network.unwrap();
	assert_eq!(token_network.address, token_network_address);
	assert_eq!(token_network.token_address, chain_info.token_address);

	let token_network = views::get_token_network_by_token_address(
		&result.new_state,
		chain_info.token_network_registry_address,
		chain_info.token_address,
	);
	assert!(token_network.is_some());

	let token_network = token_network.unwrap();
	assert_eq!(token_network.address, token_network_address);
	assert_eq!(token_network.token_address, chain_info.token_address);
}
