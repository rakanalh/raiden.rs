#![warn(clippy::missing_docs_in_private_items)]

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
	traits::Checksum,
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

impl ToString for CanonicalIdentifier {
	fn to_string(&self) -> String {
		format!(
			"ChainID: {}, TokenNetworkAddress: {}, ChannelID: {}",
			self.chain_identifier,
			self.token_network_address.checksum(),
			self.channel_identifier
		)
	}
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct QueueIdentifier {
	pub recipient: Address,
	pub canonical_identifier: CanonicalIdentifier,
}

impl ToString for QueueIdentifier {
	fn to_string(&self) -> String {
		format!(
			"Recipient: {}, {}",
			self.recipient.checksum(),
			self.canonical_identifier.to_string()
		)
	}
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

impl From<MessageTypeId> for [u8; 1] {
	fn from(val: MessageTypeId) -> Self {
		(val as u8).to_be_bytes()
	}
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct AddressMetadata {
	pub user_id: String,
	pub displayname: String,
	pub capabilities: String,
}

#[derive(Clone, Serialize)]
pub struct DefaultAddresses {
	pub contracts_version: String,
	#[serde(rename = "token_network_registry_address")]
	pub token_network_registry: Address,
	#[serde(rename = "secret_registry_address")]
	pub secret_registry: Address,
	#[serde(rename = "one_to_n_address")]
	pub one_to_n: Address,
	#[serde(rename = "service_registry_address")]
	pub service_registry: Address,
	#[serde(rename = "user_deposit_address")]
	pub user_deposit: Address,
	#[serde(rename = "monitoring_service_address")]
	pub monitoring_service: Address,
}
