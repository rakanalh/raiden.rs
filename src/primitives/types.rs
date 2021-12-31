use web3::types::{
    Bytes,
    H256,
    U256,
};

use crate::primitives::U64;

pub type BalanceProofData = (Locksroot, Nonce, TokenAmount, LockedAmount);

pub type BalanceHash = H256;

pub type BlockExpiration = U64;

pub type BlockNumber = U64;

pub type BlockHash = H256;

pub type BlockTimeout = u32;

pub type ChannelIdentifier = U256;

pub type EncodedLock = H256;

pub type FeeAmount = U256;

pub type GasLimit = U256;

pub type GasPrice = U256;

pub type LockedAmount = U256;

pub type LockTimeout = U64;

pub type Locksroot = Bytes;

pub type MessageIdentifier = u32;

pub type MessageHash = H256;

pub type Nonce = U256;

pub type PaymentIdentifier = U64;

pub type ProportionalFeeAmount = U256;

pub type RevealTimeout = U64;

pub type Signature = H256;

pub type RawSecret = Bytes;

pub type RetryTimeout = u64;

pub type Secret = H256;

pub type SecretHash = H256;

pub type SettleTimeout = U64;

pub type TokenAmount = U256;

pub type TransactionHash = H256;
