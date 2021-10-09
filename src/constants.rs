use crate::{
    primitives::BlockTimeout,
    state_machine::types::ChannelStatus,
};

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

pub const DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS: BlockTimeout = 5;

pub const TRANSACTION_GAS_LIMIT_UPPER_BOUND: u64 = 1_256_636; // int(0.4 * 3_141_592);
pub const TRANSACTION_INTRINSIC_GAS: u64 = 21_000;
pub const UNLOCK_TX_GAS_LIMIT: u64 = TRANSACTION_GAS_LIMIT_UPPER_BOUND;

pub const GAS_RESERVE_ESTIMATE_SECURITY_FACTOR: f64 = 1.1;

pub const CHANNEL_STATES_PRIOR_TO_CLOSE: [ChannelStatus; 2] = [ChannelStatus::Opened, ChannelStatus::Closing];

pub const MAXIMUM_PENDING_TRANSFERS: usize = 160;
