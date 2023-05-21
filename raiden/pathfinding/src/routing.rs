use std::{
	collections::HashMap,
	sync::Arc,
};

use raiden_primitives::types::{
	Address,
	AddressMetadata,
	BlockNumber,
	ChannelIdentifier,
	OneToNAddress,
	TokenAmount,
	TokenNetworkAddress,
	U256,
};
use raiden_state_machine::{
	types::{
		ChainState,
		ChannelState,
		ChannelStatus,
		RouteState,
	},
	views,
};

use crate::{
	PFSPath,
	RoutingError,
	PFS,
};

pub async fn get_best_routes(
	pfs: Arc<PFS>,
	chain_state: ChainState,
	our_address_metadata: AddressMetadata,
	token_network_address: TokenNetworkAddress,
	one_to_n_address: Option<OneToNAddress>,
	from_address: Address,
	to_address: Address,
	amount: U256,
	previous_address: Option<Address>,
) -> Result<(Vec<RouteState>, String), RoutingError> {
	let token_network =
		match views::get_token_network_by_address(&chain_state, token_network_address) {
			Some(token_network) => token_network,
			None => return Err(RoutingError::TokenNetworkUnknown),
		};

	// Always use a direct channel if available:
	// - There are no race conditions and the capacity is guaranteed to be available.
	// - There will be no mediation fees
	// - The transfer will be faster
	if token_network.partneraddresses_to_channelidentifiers.contains_key(&to_address) {
		for channel_id in token_network.partneraddresses_to_channelidentifiers[&to_address].iter() {
			let channel_state = &token_network.channelidentifiers_to_channels[&channel_id];

			// Direct channels don't have fees.
			let payment_with_fee_amount = amount;
			if channel_state.is_usable_for_new_transfer(payment_with_fee_amount, None) {
				let mut address_to_address_metadata = HashMap::new();
				address_to_address_metadata.insert(from_address, our_address_metadata.clone());

				let metadata =
					super::query_address_metadata(pfs.config.url.clone(), to_address).await?;
				address_to_address_metadata.insert(to_address, metadata);

				return Ok((
					vec![RouteState {
						route: vec![from_address, to_address],
						address_to_metadata: address_to_address_metadata,
						swaps: HashMap::default(),
						estimated_fee: TokenAmount::zero(),
					}],
					String::new(),
				))
			}
		}
	}

	let one_to_n_address = one_to_n_address.ok_or(RoutingError::PFServiceUnusable)?;

	// Does any channel have sufficient capacity for the payment?
	let usable_channels: Vec<&ChannelState> = token_network
		.partneraddresses_to_channelidentifiers
		.values()
		.map(|channels: &Vec<ChannelIdentifier>| {
			channels
				.iter()
				.map(|channel_id| &token_network.channelidentifiers_to_channels[channel_id])
				.filter(|channel: &&ChannelState| channel.is_usable_for_new_transfer(amount, None))
				.collect::<Vec<&ChannelState>>()
		})
		.flatten()
		.collect();

	if usable_channels.is_empty() {
		return Err(RoutingError::NoUsableChannels)
	}

	let latest_channel_opened_at = token_network
		.channelidentifiers_to_channels
		.values()
		.map(|channel_state| channel_state.open_transaction.finished_block_number)
		.max()
		.flatten()
		.unwrap_or_default();

	let (pfs_routes, pfs_feedback_token) = get_best_routes_pfs(
		pfs,
		chain_state,
		token_network_address,
		one_to_n_address,
		from_address,
		to_address,
		amount,
		previous_address,
		latest_channel_opened_at,
	)
	.await?;

	Ok((pfs_routes, pfs_feedback_token))
}

pub async fn get_best_routes_pfs(
	pfs: Arc<PFS>,
	chain_state: ChainState,
	token_network_address: TokenNetworkAddress,
	one_to_n_address: OneToNAddress,
	from_address: Address,
	to_address: Address,
	amount: TokenAmount,
	previous_address: Option<Address>,
	pfs_wait_for_block: BlockNumber,
) -> Result<(Vec<RouteState>, String), RoutingError> {
	let (routes, feedback_token) = pfs
		.query_paths(
			chain_state.our_address,
			token_network_address,
			one_to_n_address,
			chain_state.block_number,
			from_address,
			to_address,
			amount,
			pfs_wait_for_block,
		)
		.await?;

	let mut paths = vec![];
	for route in routes {
		if let Some(route_state) =
			make_route_state(route, previous_address, chain_state.clone(), token_network_address)
		{
			paths.push(route_state)
		}
	}

	Ok((paths, feedback_token))
}

pub fn make_route_state(
	route: PFSPath,
	previous_address: Option<Address>,
	chain_state: ChainState,
	token_network_address: TokenNetworkAddress,
) -> Option<RouteState> {
	if route.path.len() < 2 {
		return None
	}

	let partner_address = route.path[1];
	// Prevent back routing
	if let Some(previous_address) = previous_address {
		if partner_address == previous_address {
			return None
		}
	}

	let channel_state = match views::get_channel_by_token_network_and_partner(
		&chain_state,
		token_network_address,
		partner_address,
	) {
		Some(channel_state) => channel_state,
		None => return None,
	};

	if channel_state.status() != ChannelStatus::Opened {
		return None
	}

	return Some(RouteState {
		route: route.path,
		address_to_metadata: route.address_metadata,
		swaps: HashMap::new(),
		estimated_fee: route.estimated_fee,
	})
}
