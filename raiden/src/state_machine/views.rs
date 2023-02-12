use std::{cmp::max, collections::HashMap};

use web3::types::{Address, U256};

use crate::{
	primitives::{
		AddressMetadata, CanonicalIdentifier, TokenAddress, TokenAmount, TokenNetworkAddress,
		TokenNetworkRegistryAddress,
	},
	state_machine::views,
};

use super::types::{
	ChainState, ChannelEndState, ChannelState, ChannelStatus, RouteState,
	TokenNetworkRegistryState, TokenNetworkState,
};

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
	token_network_address: TokenNetworkAddress,
) -> Option<&TokenNetworkRegistryState> {
	let token_network_registries = &chain_state.identifiers_to_tokennetworkregistries;
	for token_network_registry in token_network_registries.values() {
		let token_network = token_network_registry
			.tokennetworkaddresses_to_tokennetworks
			.get(&token_network_address);
		if token_network.is_some() {
			return Some(token_network_registry)
		}
	}
	None
}

pub fn get_token_network_by_address(
	chain_state: &ChainState,
	token_network_address: TokenNetworkAddress,
) -> Option<&TokenNetworkState> {
	let token_network_registries = &chain_state.identifiers_to_tokennetworkregistries;
	token_network_registries
		.values()
		.map(|tnr| tnr.tokennetworkaddresses_to_tokennetworks.values())
		.flatten()
		.find(|tn| tn.address == token_network_address)
}

pub fn get_token_network_by_token_address(
	chain_state: &ChainState,
	registry_address: Address,
	token_address: TokenAddress,
) -> Option<&TokenNetworkState> {
	let token_network_registry =
		match chain_state.identifiers_to_tokennetworkregistries.get(&registry_address) {
			Some(tnr) => tnr,
			None => return None,
		};

	token_network_registry
		.tokennetworkaddresses_to_tokennetworks
		.values()
		.find(|tn| tn.token_address == token_address)
}

pub fn get_channels(chain_state: &ChainState) -> Vec<ChannelState> {
	let mut channels = vec![];

	for token_network_registry in chain_state.identifiers_to_tokennetworkregistries.values() {
		for token_network in token_network_registry.tokennetworkaddresses_to_tokennetworks.values()
		{
			channels.extend(token_network.channelidentifiers_to_channels.values().cloned());
		}
	}

	channels
}

pub fn get_channel_by_canonical_identifier(
	chain_state: &ChainState,
	canonical_identifier: CanonicalIdentifier,
) -> Option<&ChannelState> {
	let token_network =
		get_token_network_by_address(chain_state, canonical_identifier.token_network_address);
	if let Some(token_network) = token_network {
		return token_network
			.channelidentifiers_to_channels
			.get(&canonical_identifier.channel_identifier)
	}
	None
}

pub fn get_channel_by_token_network_and_partner(
	chain_state: &ChainState,
	token_network_address: TokenNetworkAddress,
	partner_address: Address,
) -> Option<&ChannelState> {
	let token_network = get_token_network_by_address(chain_state, token_network_address);
	if let Some(token_network) = token_network {
		return token_network
			.channelidentifiers_to_channels
			.values()
			.find(|channel| channel.partner_state.address == partner_address)
	}
	None
}

pub fn get_channel_state_for(
	chain_state: &ChainState,
	registry_address: Address,
	token_address: TokenAddress,
	partner_address: Address,
) -> Option<&ChannelState> {
	match get_token_network_by_token_address(chain_state, registry_address, token_address) {
		Some(token_network) => token_network
			.channelidentifiers_to_channels
			.iter()
			.map(|(_, c)| c)
			.find(|c| c.partner_state.address == partner_address),
		_ => None,
	}
}

pub fn channel_distributable(sender: &ChannelEndState, receiver: &ChannelEndState) -> TokenAmount {
	let (_, _, transferred_amount, locked_amount) = sender.get_current_balanceproof();
	let distributable = channel_balance(sender, receiver) - sender.locked_amount();
	let overflow_limit = TokenAmount::MAX - transferred_amount - locked_amount;
	TokenAmount::min(overflow_limit, distributable)
}

pub fn channel_balance(sender: &ChannelEndState, receiver: &ChannelEndState) -> U256 {
	let mut sender_transferred_amount = U256::zero();
	let mut receiver_transferred_amount = U256::zero();

	if let Some(balance_proof) = &sender.balance_proof {
		sender_transferred_amount = balance_proof.transferred_amount;
	}
	if let Some(balance_proof) = &receiver.balance_proof {
		receiver_transferred_amount = balance_proof.transferred_amount;
	}

	sender.contract_balance -
		max(sender.offchain_total_withdraw(), sender.onchain_total_withdraw) -
		sender_transferred_amount +
		receiver_transferred_amount
}

pub fn get_token_identifiers(chain_state: &ChainState, registry_address: Address) -> Vec<Address> {
	match chain_state.identifiers_to_tokennetworkregistries.get(&registry_address) {
		Some(registry) =>
			registry.tokenaddresses_to_tokennetworkaddresses.keys().cloned().collect(),
		None => vec![],
	}
}

fn get_channelstate_filter(
	chain_state: &ChainState,
	token_network_registry_address: TokenNetworkRegistryAddress,
	token_address: TokenAddress,
	filter_fn: fn(&ChannelState) -> bool,
) -> Vec<ChannelState> {
	let token_network = match views::get_token_network_by_token_address(
		chain_state,
		token_network_registry_address,
		token_address,
	) {
		Some(token_network) => token_network,
		None => return vec![],
	};

	let mut result = vec![];

	for channel_state in token_network.channelidentifiers_to_channels.values() {
		if filter_fn(channel_state) {
			result.push(channel_state.clone())
		}
	}

	return result
}

pub fn get_channelstate_open(
	chain_state: &ChainState,
	registry_address: Address,
	token_address: TokenAddress,
) -> Vec<ChannelState> {
	return get_channelstate_filter(chain_state, registry_address, token_address, |channel_state| {
		channel_state.status() == ChannelStatus::Opened
	})
}

pub fn get_channelstate_closing(
	chain_state: &ChainState,
	registry_address: Address,
	token_address: TokenAddress,
) -> Vec<ChannelState> {
	return get_channelstate_filter(chain_state, registry_address, token_address, |channel_state| {
		channel_state.status() == ChannelStatus::Closing
	})
}

pub fn get_channelstate_closed(
	chain_state: &ChainState,
	registry_address: Address,
	token_address: TokenAddress,
) -> Vec<ChannelState> {
	return get_channelstate_filter(chain_state, registry_address, token_address, |channel_state| {
		channel_state.status() == ChannelStatus::Closed
	})
}

pub fn get_channelstate_settling(
	chain_state: &ChainState,
	registry_address: Address,
	token_address: TokenAddress,
) -> Vec<ChannelState> {
	return get_channelstate_filter(chain_state, registry_address, token_address, |channel_state| {
		channel_state.status() == ChannelStatus::Settling
	})
}

pub fn get_channelstate_settled(
	chain_state: &ChainState,
	registry_address: Address,
	token_address: TokenAddress,
) -> Vec<ChannelState> {
	return get_channelstate_filter(chain_state, registry_address, token_address, |channel_state| {
		channel_state.status() == ChannelStatus::Settled
	})
}

pub fn get_addresses_to_channels(
	chain_state: &ChainState,
) -> HashMap<(TokenNetworkAddress, Address), &ChannelState> {
	let mut channels = HashMap::new();

	for token_network_registry in chain_state.identifiers_to_tokennetworkregistries.values() {
		for token_network in token_network_registry.tokennetworkaddresses_to_tokennetworks.values()
		{
			for channel in token_network.channelidentifiers_to_channels.values() {
				channels.insert((token_network.address, channel.partner_state.address), channel);
			}
		}
	}

	channels
}

pub fn get_address_metadata(
	recipient_address: Address,
	route_states: Vec<RouteState>,
) -> Option<AddressMetadata> {
	for route_state in route_states {
		match route_state.address_to_metadata.get(&recipient_address) {
			Some(metadata) => return Some(metadata.clone()),
			None => continue,
		};
	}

	None
}
