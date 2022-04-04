use lazy_static::lazy_static;
use web3::{
    signing::keccak256,
    types::{
        Address,
        Bytes,
    },
};

use crate::{
    primitives::{
        CanonicalIdentifier,
        ChainID,
        ChannelIdentifier,
    },
    state_machine::types::{
        ChannelStatus,
        PayeeState,
        PayerState,
    },
};

pub const ABSENT_SECRET: Bytes = Bytes(vec![]);
pub const SECRET_LENGTH: u8 = 32;

pub const MIN_REVEAL_TIMEOUT: u32 = 1;
pub const DEFAULT_REVEAL_TIMEOUT: u32 = 50;
pub const DEFAULT_SETTLE_TIMEOUT: u32 = 500;
pub const DEFAULT_RETRY_TIMEOUT: u64 = 500;

pub const SNAPSHOT_STATE_CHANGE_COUNT: u16 = 500;

pub const DEFAULT_MEDIATION_FLAT_FEE: u64 = 0;
pub const DEFAULT_MEDIATION_PROPORTIONAL_FEE: u64 = 4000; // 0.4% in parts per million
pub const DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE: u64 = 3000; // 0.3% in parts per million
pub const NUM_DISCRETISATION_POINTS: u64 = 21;

pub const DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS: u64 = 5;
pub const DEFAULT_WAIT_BEFORE_LOCK_REMOVAL: u64 = 2 * DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS;

pub const TRANSACTION_GAS_LIMIT_UPPER_BOUND: u64 = 1_256_636; // int(0.4 * 3_141_592);
pub const TRANSACTION_INTRINSIC_GAS: u64 = 21_000;
pub const UNLOCK_TX_GAS_LIMIT: u64 = TRANSACTION_GAS_LIMIT_UPPER_BOUND;

pub const GAS_RESERVE_ESTIMATE_SECURITY_FACTOR: f64 = 1.1;

pub const MAXIMUM_PENDING_TRANSFERS: usize = 160;

// 0.2%
pub const MAX_MEDIATION_FEE_PERC: (u32, u32) = (2, 1000);
// 0.03%
pub const DEFAULT_MEDIATION_FEE_MARGIN: (u32, u32) = (3, 10000);
// 0.0005%
pub const PAYMENT_AMOUNT_BASED_FEE_MARGIN: (u32, u32) = (5, 100000);

pub const CHANNEL_STATES_PRIOR_TO_CLOSE: [ChannelStatus; 2] = [ChannelStatus::Opened, ChannelStatus::Closing];
pub const CHANNEL_STATES_UP_TO_CLOSE: [ChannelStatus; 3] =
    [ChannelStatus::Opened, ChannelStatus::Closing, ChannelStatus::Closed];
pub const PAYEE_STATE_TRANSFER_PAID: [PayeeState; 2] = [PayeeState::BalanceProof, PayeeState::ContractUnlock];
pub const PAYER_STATE_TRANSFER_PAID: [PayerState; 1] = [PayerState::BalanceProof];

pub const PAYEE_STATE_TRANSFER_FINAL: [PayeeState; 3] = [
    PayeeState::ContractUnlock,
    PayeeState::BalanceProof,
    PayeeState::Expired,
];
pub const PAYER_STATE_TRANSFER_FINAL: [PayerState; 2] = [PayerState::BalanceProof, PayerState::Expired];

pub const CANONICAL_IDENTIFIER_UNORDERED_QUEUE: CanonicalIdentifier = CanonicalIdentifier {
    chain_identifier: ChainID::Mainnet,
    token_network_address: Address::zero(),
    channel_identifier: ChannelIdentifier::zero(),
};

lazy_static! {
    pub static ref LOCKSROOT_OF_NO_LOCKS: Vec<u8> = keccak256(&[]).to_vec();
}
