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

/// Chain identifier module.
mod chain_id;
pub use chain_id::*;

/// Custom numeric data types.
mod numeric;
pub use numeric::*;

use crate::{
	deserializers::u256_from_str,
	serializers::u256_to_str,
	traits::Checksum,
};

/// Alias type for BalanceProofData
pub type BalanceProofData = (Locksroot, Nonce, TokenAmount, LockedAmount);

// Alias type for Balance Hash
pub type BalanceHash = H256;

/// Alias type for block expiration.
pub type BlockExpiration = U64;

/// Alias type for block number.
pub type BlockNumber = U64;

/// Alias type for block hash.
pub type BlockHash = H256;

/// Alias type for block timeout.
pub type BlockTimeout = U64;

/// Alias type for channel identifier.
pub type ChannelIdentifier = U256;

/// Alias type for encoded lock.
pub type EncodedLock = Bytes;

/// Alias type for fee amount.
pub type FeeAmount = U256;

/// Alias type for gas limit.
pub type GasLimit = U256;

/// Alias type for gas price.
pub type GasPrice = U256;

/// Alias price for locked amount.
pub type LockedAmount = U256;

/// Alias type for lock timeout.
pub type LockTimeout = U64;

/// Alias type for locksroot.
pub type Locksroot = H256;

/// Alias type for message identifier.
pub type MessageIdentifier = u64;

/// Alias type for message hash.
pub type MessageHash = H256;

/// Alias type for nonce.
pub type Nonce = U256;

/// Alias type for OneToN address.
pub type OneToNAddress = Address;

/// Alias type for payment identifier.
pub type PaymentIdentifier = U64;

/// Alias type for proportional fee amount.
pub type ProportionalFeeAmount = U256;

/// Alias type for reveal timeout.
pub type RevealTimeout = U64;

/// Alias type for retry timeout.
pub type RetryTimeout = u64;

/// Alias type for secret.
pub type Secret = Bytes;

/// Alias type for secret hash.
pub type SecretHash = H256;

/// ALias type for secret registry address.
pub type SecretRegistryAddress = Address;

/// Alias type for signature.
pub type Signature = Bytes;

/// Alias type for settle timeout.
pub type SettleTimeout = U64;

/// Alias type for token address.
pub type TokenAddress = Address;

/// Alias type for token network registry address.
pub type TokenNetworkRegistryAddress = Address;

/// Alias type for token network address.
pub type TokenNetworkAddress = Address;

/// Alias type for token amount.
pub type TokenAmount = U256;

/// Alias type for transaction hash.
pub type TransactionHash = H256;

/// The channel's canonical identifier.
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

/// Message queue identifier.
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

/// Message type identifier.
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

/// Networking address metadata
#[derive(Clone, Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct AddressMetadata {
	pub user_id: String,
	pub displayname: String,
	pub capabilities: String,
}

/// Contains a list of deployed contract addresses vital for the operation of the node.
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
