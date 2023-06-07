use std::{
	convert::TryInto,
	sync::Arc,
};

use raiden_primitives::types::{
	DefaultAddresses,
	H160,
	H256,
	U64,
};
use raiden_state_machine::types::ChainState;
use web3::types::{
	Filter,
	FilterBuilder,
};

use super::contracts::{
	ContractIdentifier,
	ContractsManager,
};

/// Returns the current filter for syncing with the blockchain according to the latest chain state.
pub fn filters_from_chain_state(
	default_addresses: DefaultAddresses,
	contracts_manager: Arc<ContractsManager>,
	chain_state: ChainState,
	from_block: U64,
	to_block: U64,
) -> Filter {
	let token_network_registries = chain_state.identifiers_to_tokennetworkregistries.values();
	let token_networks = token_network_registries
		.clone()
		.flat_map(|tnr| tnr.tokennetworkaddresses_to_tokennetworks.values());

	let token_network_registry_contract: ethabi::Contract = contracts_manager
		.get(ContractIdentifier::TokenNetworkRegistry)
		.try_into()
		.unwrap();
	let token_network_contract: ethabi::Contract =
		contracts_manager.get(ContractIdentifier::TokenNetwork).try_into().unwrap();

	let service_registry_contract: ethabi::Contract =
		contracts_manager.get(ContractIdentifier::TokenNetwork).try_into().unwrap();

	let mut addresses: Vec<H160> = token_network_registries.map(|t| t.address).collect();
	addresses.extend(token_networks.map(|tn| tn.address));
	addresses.push(default_addresses.service_registry);

	let mut topics: Vec<H256> =
		token_network_registry_contract.events().map(|e| e.signature()).collect();
	topics.extend(token_network_contract.events().map(|e| e.signature()));
	topics.extend(service_registry_contract.events().map(|e| e.signature()));

	FilterBuilder::default()
		.address(addresses)
		.topics(Some(topics), None, None, None)
		.from_block(from_block.into())
		.to_block(to_block.into())
		.build()
}
