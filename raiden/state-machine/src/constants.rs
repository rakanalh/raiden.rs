use raiden_primitives::types::Bytes;

use crate::types::{
	ChannelStatus,
	PayeeState,
	PayerState,
};

pub const ABSENT_SECRET: Bytes = Bytes(vec![]);

pub const SECRET_LENGTH: u8 = 32;

pub const MIN_REVEAL_TIMEOUT: u32 = 1;

pub const DEFAULT_REVEAL_TIMEOUT: u32 = 50;

pub const DEFAULT_SETTLE_TIMEOUT: u32 = 500;

pub const DEFAULT_RETRY_TIMEOUT: u64 = 500;

pub const DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS: u64 = 5;

pub const DEFAULT_WAIT_BEFORE_LOCK_REMOVAL: u64 = 2 * DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS;

pub const MAXIMUM_PENDING_TRANSFERS: usize = 160;

pub const NUM_DISCRETISATION_POINTS: u64 = 21;

pub const MAX_MEDIATION_FEE_PERC: (u32, u32) = (20, 100);

pub const DEFAULT_MEDIATION_FEE_MARGIN: (u32, u32) = (3, 100);

pub const PAYMENT_AMOUNT_BASED_FEE_MARGIN: (u32, u32) = (5, 10000);

pub const DEFAULT_MEDIATION_FLAT_FEE: u64 = 0;

pub const DEFAULT_MEDIATION_PROPORTIONAL_FEE: u64 = 4000; // 0.4% in parts per million

pub const DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE: u64 = 3000; // 0.3% in parts per million

pub const CHANNEL_STATES_PRIOR_TO_CLOSE: [ChannelStatus; 2] =
	[ChannelStatus::Opened, ChannelStatus::Closing];
pub const CHANNEL_STATES_UP_TO_CLOSE: [ChannelStatus; 3] =
	[ChannelStatus::Opened, ChannelStatus::Closing, ChannelStatus::Closed];
pub const PAYEE_STATE_TRANSFER_PAID: [PayeeState; 2] =
	[PayeeState::BalanceProof, PayeeState::ContractUnlock];
pub const PAYER_STATE_TRANSFER_PAID: [PayerState; 1] = [PayerState::BalanceProof];

pub const PAYEE_STATE_TRANSFER_FINAL: [PayeeState; 3] =
	[PayeeState::ContractUnlock, PayeeState::BalanceProof, PayeeState::Expired];
pub const PAYEE_STATE_SECRET_KNOWN: [PayeeState; 3] =
	[PayeeState::SecretRevealed, PayeeState::ContractUnlock, PayeeState::BalanceProof];
pub const PAYER_STATE_SECRET_KNOWN: [PayerState; 3] =
	[PayerState::SecretRevealed, PayerState::WaitingUnlock, PayerState::BalanceProof];
pub const PAYER_STATE_TRANSFER_FINAL: [PayerState; 2] =
	[PayerState::BalanceProof, PayerState::Expired];
