use crate::{
    primitives::{
        BlockTimeout,
        FeeAmount,
        ProportionalFeeAmount,
    },
    state_machine::types::ChannelStatus,
};

pub const MIN_REVEAL_TIMEOUT: u32 = 1;
pub const DEFAULT_REVEAL_TIMEOUT: u32 = 50;
pub const DEFAULT_SETTLE_TIMEOUT: u32 = 500;
pub const DEFAULT_RETRY_TIMEOUT: f32 = 0.5;

pub const SNAPSHOT_STATE_CHANGE_COUNT: u16 = 500;

pub const DEFAULT_MEDIATION_FLAT_FEE: FeeAmount = 0;
pub const DEFAULT_MEDIATION_PROPORTIONAL_FEE: ProportionalFeeAmount = 4000; // 0.4% in parts per million
pub const DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE: ProportionalFeeAmount = 3000; // 0.3% in parts per million

pub const DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS: BlockTimeout = 5;

pub const CHANNEL_STATES_PRIOR_TO_CLOSE: [ChannelStatus; 2] = [ChannelStatus::Opened, ChannelStatus::Closing];
