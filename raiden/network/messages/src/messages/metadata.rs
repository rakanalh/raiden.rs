use std::collections::HashMap;

use raiden_blockchain::secret::encrypt_secret;
use raiden_primitives::types::{
	Address,
	AddressMetadata,
	Secret,
};
use raiden_state_machine::{
	types::SendLockedTransfer,
	views::get_address_metadata,
};
use serde::{
	Deserialize,
	Serialize,
};
use web3::signing::keccak256;

/// Contains the metadata of the transfer route.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteMetadata {
	pub route: Vec<Address>,
	pub address_metadata: HashMap<Address, AddressMetadata>,
}

/// Metadata is used by nodes to provide following hops in a transfer with additional information,
/// e.g. to make additonal optimizations possible.
/// It can contain arbitrary data and should be considered a read-only datastructure
/// for mediating nodes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
	pub routes: Vec<RouteMetadata>,
	pub secret: Option<Secret>,
}

impl Metadata {
	pub fn hash(&self) -> Result<Vec<u8>, String> {
		let value = serde_json::to_value(self)
			.map_err(|e| format!("Could not convert metadata to JSON: {:?}", e))?;
		let data = canonical_json::to_string(&value)
			.map_err(|e| format!("Could not canonicalize json: {:?}", e))?;

		let mut res: Vec<u8> = Vec::new();
		res.extend_from_slice(data.as_bytes());

		Ok(keccak256(&res).to_vec())
	}
}

impl From<SendLockedTransfer> for Metadata {
	fn from(event: SendLockedTransfer) -> Self {
		let transfer = event.transfer.clone();
		let routes: Vec<RouteMetadata> = transfer
			.route_states
			.into_iter()
			.map(|r| RouteMetadata { route: r.route, address_metadata: r.address_to_metadata })
			.collect();

		let target_metadata = get_address_metadata(transfer.target, event.transfer.route_states);
		let secret = match target_metadata {
			Some(target_metadata) => transfer.secret.map(|s| {
				encrypt_secret(
					s,
					target_metadata,
					transfer.lock.amount,
					transfer.payment_identifier,
				)
				.unwrap()
			}),
			None => None,
		};
		Self { routes, secret }
	}
}
