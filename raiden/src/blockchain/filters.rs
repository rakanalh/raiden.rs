use std::{convert::TryInto, sync::Arc};

use web3::types::{Filter, FilterBuilder, H160, H256};

use crate::{primitives::U64, state_machine::types::ChainState};

use super::contracts::{ContractIdentifier, ContractsManager};

pub fn filters_from_chain_state(
	contracts_manager: Arc<ContractsManager>,
	chain_state: ChainState,
	from_block: U64,
	to_block: U64,
) -> Filter {
	let token_network_registries = chain_state.identifiers_to_tokennetworkregistries.values();
	let token_networks = token_network_registries
		.clone()
		.flat_map(|tnr| tnr.tokennetworkaddresses_to_tokennetworks.values());
	println!("Filters: {:?}", token_networks);
	let _channels =
		token_networks.clone().flat_map(|tn| tn.channelidentifiers_to_channels.values());

	let tnr_contract: ethabi::Contract = contracts_manager
		.get(ContractIdentifier::TokenNetworkRegistry)
		.try_into()
		.unwrap();
	let tn_contract: ethabi::Contract =
		contracts_manager.get(ContractIdentifier::TokenNetwork).try_into().unwrap();

	let mut addresses: Vec<H160> = token_network_registries.map(|t| t.address).collect();
	addresses.extend(token_networks.map(|tn| tn.address));

	let mut topics: Vec<H256> = tnr_contract.events().map(|e| e.signature()).collect();
	topics.extend(tn_contract.events().map(|e| e.signature()));

	FilterBuilder::default()
		.address(addresses)
		.topics(Some(topics), None, None, None)
		.from_block(from_block.into())
		.to_block(to_block.into())
		.build()
}
