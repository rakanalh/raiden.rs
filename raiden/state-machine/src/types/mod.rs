mod event;
mod state;
mod state_change;

use raiden_primitives::types::{
	Address,
	ChainID,
	PaymentIdentifier,
	Secret,
	TokenAmount,
	TokenNetworkAddress,
	U256,
	U64,
};
use rand_chacha::{
	rand_core::{
		RngCore,
		SeedableRng,
	},
	ChaChaRng,
};
use serde::{
	Deserialize,
	Serialize,
};

pub use self::{
	event::*,
	state::*,
	state_change::*,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Random(ChaChaRng);

impl Random {
	pub fn new() -> Self {
		Self(ChaChaRng::seed_from_u64(0))
	}

	pub fn next(&mut self) -> u64 {
		self.0.next_u64()
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct AddressMetadata {
	pub user_id: String,
	pub displayname: String,
	pub capabilities: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CanonicalIdentifier {
	pub chain_identifier: ChainID,
	pub token_network_address: TokenNetworkAddress,
	pub channel_identifier: U256,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct QueueIdentifier {
	pub recipient: Address,
	pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum TransactionResult {
	Success,
	Failure,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct TransactionExecutionStatus {
	pub started_block_number: Option<U64>,
	pub finished_block_number: Option<U64>,
	pub result: Option<TransactionResult>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct DecryptedSecret {
	pub secret: Secret,
	pub amount: TokenAmount,
	pub payment_identifier: PaymentIdentifier,
}
