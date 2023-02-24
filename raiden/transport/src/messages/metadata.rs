use std::collections::HashMap;

use raiden_primitives::types::{
	Address,
	Secret,
};
use raiden_state_machine::{
	types::{
		AddressMetadata,
		SendLockedTransfer,
	},
	views::get_address_metadata,
};
use serde::{
	Deserialize,
	Serialize,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct RouteMetadata {
	route: Vec<Address>,
	address_metadata: Option<HashMap<Address, AddressMetadata>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Metadata {
	routes: Vec<RouteMetadata>,
	secret: Option<Secret>,
}

impl From<SendLockedTransfer> for Metadata {
	fn from(event: SendLockedTransfer) -> Self {
		let transfer = event.transfer.clone();
		let routes: Vec<RouteMetadata> = transfer
			.route_states
			.into_iter()
			.map(|r| RouteMetadata {
				route: r.route,
				address_metadata: Some(r.address_to_metadata),
			})
			.collect();
		let target_metadata =
			get_address_metadata(transfer.target, event.transfer.route_states.clone());
		Self { routes, secret: transfer.secret }
	}
}
