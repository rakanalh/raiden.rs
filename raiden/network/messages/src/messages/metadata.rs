use std::collections::HashMap;

use raiden_primitives::types::{
	Address,
	Secret,
	H256,
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
use tiny_keccak::{
	Hasher,
	Keccak,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteMetadata {
	pub route: Vec<Address>,
	pub address_metadata: HashMap<Address, AddressMetadata>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

		let mut keccak = Keccak::v256();
		let mut result = [0u8; 32];
		keccak.update(&res);
		keccak.finalize(&mut result);
		Ok(result.to_vec())
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
		let _target_metadata =
			get_address_metadata(transfer.target, event.transfer.route_states.clone());
		Self { routes, secret: transfer.secret }
	}
}
