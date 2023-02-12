mod config;
mod iou;
mod keys;
mod numeric;
pub mod signature;
mod types;

pub use config::*;
pub use iou::*;
pub use keys::*;
pub use numeric::*;
pub use types::*;

use derive_more::Display;
use rand_chacha::{
	rand_core::{RngCore, SeedableRng},
	ChaChaRng,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, str::FromStr};
use web3::types::{Address, U256};

use crate::constants::{
	DEFAULT_MEDIATION_FLAT_FEE, DEFAULT_MEDIATION_PROPORTIONAL_FEE,
	DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Random(ChaChaRng);

impl Random {
	pub fn new() -> Self {
		Self(ChaChaRng::seed_from_u64(0))
	}

	pub fn next(&mut self) -> u32 {
		self.0.next_u32()
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct AddressMetadata {
	pub user_id: String,
	pub displayname: String,
	pub capabilities: HashMap<String, String>,
}

#[repr(u8)]
#[derive(Copy, Clone, Display, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ChainID {
	Mainnet = 1,
	Ropsten = 3,
	Rinkeby = 4,
	Goerli = 5,
	Private = 6,
}

impl Into<U256> for ChainID {
	fn into(self) -> U256 {
		(self as u32).into()
	}
}

impl Into<Vec<u8>> for ChainID {
	fn into(self) -> Vec<u8> {
		(self as u8).to_be_bytes().to_vec()
	}
}

impl FromStr for ChainID {
	type Err = ();

	fn from_str(s: &str) -> Result<ChainID, ()> {
		match s {
			"mainnet" => Ok(ChainID::Mainnet),
			"ropsten" => Ok(ChainID::Ropsten),
			"rinkeby" => Ok(ChainID::Rinkeby),
			"goerli" => Ok(ChainID::Goerli),
			"private" => Ok(ChainID::Private),
			_ => Err(()),
		}
	}
}

pub enum EnvironmentType {
	Production,
	Development,
}

#[derive(Copy, Clone, PartialEq)]
pub enum RoutingMode {
	PFS,
	Private,
}

#[repr(u8)]
#[derive(Clone, Display, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum MessageTypeId {
	BalanceProof = 1,
	BalanceProofUpdate = 2,
	Withdraw = 3,
	CooperativeSettle = 4,
	IOU = 5,
	MSReward = 6,
}

impl Into<[u8; 1]> for MessageTypeId {
	fn into(self) -> [u8; 1] {
		(self as u8).to_be_bytes()
	}
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
