use derive_more::Display;
use serde::{
	Deserialize,
	Serialize,
};

pub enum EnvironmentType {
	Production,
	Development,
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
