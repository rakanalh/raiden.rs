mod event;
mod state;
mod state_change;

use std::{
	collections::HashMap,
	ops::{
		Add,
		Mul,
		Sub,
	},
	str::FromStr,
};

use derive_more::{
	Deref,
	Display,
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
use web3::types::{
	Address,
	Bytes,
	H256,
	U256,
	U64 as PrimitiveU64,
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

#[derive(
	Default,
	Copy,
	Clone,
	Display,
	Debug,
	Deref,
	Eq,
	Ord,
	PartialEq,
	PartialOrd,
	Hash,
	Serialize,
	Deserialize,
)]
pub struct U64(PrimitiveU64);

impl U64 {
	pub fn zero() -> Self {
		Self(PrimitiveU64::zero())
	}

	pub fn as_bytes(&self) -> &[u8] {
		let mut bytes = vec![];
		self.0.to_big_endian(&mut bytes);

		let r: &mut [u8] = Default::default();
		r.clone_from_slice(&bytes[..]);
		r
	}
}

impl From<PrimitiveU64> for U64 {
	fn from(n: PrimitiveU64) -> Self {
		Self(n)
	}
}

impl From<U64> for PrimitiveU64 {
	fn from(n: U64) -> Self {
		n.0
	}
}

impl FromStr for U64 {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(U64(PrimitiveU64::from_str(s).map_err(|_| ())?))
	}
}

impl Add<U64> for U64 {
	type Output = U64;

	fn add(self, rhs: U64) -> Self::Output {
		U64::from(self.0 + rhs.0)
	}
}

impl Sub<U64> for U64 {
	type Output = U64;

	fn sub(self, rhs: U64) -> Self::Output {
		U64::from(self.0 - rhs.0)
	}
}

impl Mul<U64> for U64 {
	type Output = U64;

	fn mul(self, rhs: U64) -> Self::Output {
		U64::from(self.0 * rhs.0)
	}
}

impl Mul<u64> for U64 {
	type Output = U64;

	fn mul(self, rhs: u64) -> Self::Output {
		U64::from(self.0 * rhs)
	}
}

impl From<U64> for U256 {
	fn from(num: U64) -> Self {
		num.0.low_u64().into()
	}
}

impl From<u64> for U64 {
	fn from(n: u64) -> Self {
		Self(n.into())
	}
}

impl From<u32> for U64 {
	fn from(n: u32) -> Self {
		Self((n as u64).into())
	}
}

impl From<i32> for U64 {
	fn from(n: i32) -> Self {
		Self((n as u64).into())
	}
}

pub trait AmountToBytes {
	fn to_bytes(&self) -> &[u8];
}

impl AmountToBytes for TokenAmount {
	fn to_bytes(&self) -> &[u8] {
		let mut bytes = vec![];
		self.to_big_endian(&mut bytes);

		let r: &mut [u8] = Default::default();
		r.clone_from_slice(&bytes[..]);
		r
	}
}

pub type BalanceProofData = (Locksroot, Nonce, TokenAmount, LockedAmount);

pub type BalanceHash = H256;

pub type BlockExpiration = U64;

pub type BlockNumber = U64;

pub type BlockHash = H256;

pub type BlockTimeout = U64;

pub type ChannelIdentifier = U256;

pub type EncodedLock = Bytes;

pub type FeeAmount = U256;

pub type GasLimit = U256;

pub type GasPrice = U256;

pub type LockedAmount = U256;

pub type LockTimeout = U64;

pub type Locksroot = Bytes;

pub type MessageIdentifier = u32;

pub type MessageHash = H256;

pub type Nonce = U256;

pub type OneToNAddress = Address;

pub type PaymentIdentifier = U64;

pub type ProportionalFeeAmount = U256;

pub type RevealTimeout = U64;

pub type RetryTimeout = u64;

pub type Secret = Bytes;

pub type SecretHash = H256;

pub type SecretRegistryAddress = Address;

pub type Signature = H256;

pub type SettleTimeout = U64;

pub type TokenAddress = Address;

pub type TokenNetworkRegistryAddress = Address;

pub type TokenNetworkAddress = Address;

pub type TokenAmount = U256;

pub type TransactionHash = H256;
