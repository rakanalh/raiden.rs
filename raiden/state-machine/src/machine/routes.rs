#![warn(clippy::missing_docs_in_private_items)]

use std::collections::HashMap;

use raiden_primitives::types::{
	Address,
	ChannelIdentifier,
	TokenNetworkAddress,
};

use crate::types::{
	ChannelState,
	RouteState,
};

pub fn prune_route_table(
	route_states: Vec<RouteState>,
	selected_route: RouteState,
	our_address: Address,
) -> Vec<RouteState> {
	route_states
		.iter()
		.filter(|route_state| {
			route_state.hop_after(our_address) == selected_route.hop_after(our_address)
		})
		.map(|route_state| RouteState {
			route: route_state.route[1..].to_vec(),
			..route_state.clone()
		})
		.collect()
}

pub fn filter_acceptable_routes(
	route_states: Vec<RouteState>,
	blacklisted_channel_ids: Vec<ChannelIdentifier>,
	addresses_to_channels: HashMap<(TokenNetworkAddress, Address), &ChannelState>,
	token_network_address: TokenNetworkAddress,
	our_address: Address,
) -> Vec<RouteState> {
	let mut acceptable_routes = vec![];
	for route in route_states {
		let next_hop = match route.hop_after(our_address) {
			Some(next_hop) => next_hop,
			None => continue,
		};
		let channel = match addresses_to_channels.get(&(token_network_address, next_hop)) {
			Some(channel) => channel,
			None => continue,
		};
		if !blacklisted_channel_ids.contains(&channel.canonical_identifier.channel_identifier) {
			acceptable_routes.push(route);
		}
	}
	acceptable_routes
}
