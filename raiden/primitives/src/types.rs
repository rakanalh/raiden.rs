use derive_more::Display;
use serde::{
	Deserialize,
	Serialize,
};
pub use web3::types::{
	Address,
	BlockId,
	Bytes,
	H160,
	H256,
	U256,
};

mod chain_id;
pub use chain_id::*;

mod numeric;
pub use numeric::*;

use crate::{
	deserializers::u256_from_str,
	serializers::u256_to_str,
};

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

pub type Locksroot = H256;

pub type MessageIdentifier = u64;

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

pub type Signature = Bytes;

pub type SettleTimeout = U64;

pub type TokenAddress = Address;

pub type TokenNetworkRegistryAddress = Address;

pub type TokenNetworkAddress = Address;

pub type TokenAmount = U256;

pub type TransactionHash = H256;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CanonicalIdentifier {
	pub chain_identifier: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub channel_identifier: ChannelIdentifier,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct QueueIdentifier {
	pub recipient: Address,
	pub canonical_identifier: CanonicalIdentifier,
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

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct AddressMetadata {
	pub user_id: String,
	pub displayname: String,
	pub capabilities: String,
}

#[derive(Clone)]
pub struct DefaultAddresses {
	pub token_network_registry: Address,
	pub secret_registry: Address,
	pub one_to_n: Address,
	pub service_registry: Address,
	pub monitoring_service: Address,
}
