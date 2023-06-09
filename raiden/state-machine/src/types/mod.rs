#![warn(clippy::missing_docs_in_private_items)]

mod event;
mod state;
mod state_change;

use raiden_primitives::{
	deserializers::u256_from_str,
	serializers::u256_to_str,
	types::{
		BlockNumber,
		PaymentIdentifier,
		Secret,
		TokenAmount,
	},
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

/// The channel's pseudo random number generator.
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

impl Default for Random {
	fn default() -> Self {
		Self::new()
	}
}

/// Transaction result state.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum TransactionResult {
	Success,
	Failure,
}

/// The transaction execution status.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct TransactionExecutionStatus {
	pub started_block_number: Option<BlockNumber>,
	pub finished_block_number: Option<BlockNumber>,
	pub result: Option<TransactionResult>,
}

/// Type to hold a decrypted secret with metadata.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct DecryptedSecret {
	pub secret: Secret,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub amount: TokenAmount,
	pub payment_identifier: PaymentIdentifier,
}
