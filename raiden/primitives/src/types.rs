pub use web3::types::{
	Address,
	BlockId,
	Bytes,
	H160,
	H256,
	U256,
};

mod chain_id;
pub mod message_type;
pub use chain_id::*;

mod numeric;
pub use numeric::*;

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

pub type Signature = H256;

pub type SettleTimeout = U64;

pub type TokenAddress = Address;

pub type TokenNetworkRegistryAddress = Address;

pub type TokenNetworkAddress = Address;

pub type TokenAmount = U256;

pub type TransactionHash = H256;
