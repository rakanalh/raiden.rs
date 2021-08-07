use derive_more::Deref;
use serde::{
    Deserialize,
    Serialize,
};
use web3::types::{
    Address,
    H256,
    U256,
};

use crate::primitives::{
    AddressMetadata,
    CanonicalIdentifier,
    U64,
};

use super::BalanceProofState;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum Event {
    SendWithdrawExpired(SendWithdrawExpired),
    SendWithdrawRequest(SendWithdrawRequest),
    ContractSendChannelSettle(ContractSendChannelSettle),
    ContractSendChannelUpdateTransfer(ContractSendChannelUpdateTransfer),
    ContractSendChannelBatchUnlock(ContractSendChannelBatchUnlock),
    InvalidActionWithdraw(EventInvalidActionWithdraw),
    InvalidActionSetRevealTimeout(EventInvalidActionSetRevealTimeout),
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum SendMessageEvent {
    SendWithdrawExpired(SendWithdrawExpired),
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct SendMessageEventInner {
    pub recipient: Address,
    pub recipient_metadata: Option<AddressMetadata>,
    pub canonincal_identifier: CanonicalIdentifier,
    pub message_identifier: u32,
}

impl PartialEq for SendMessageEventInner {
    fn eq(&self, other: &Self) -> bool {
        self.recipient == other.recipient
            && self.recipient_metadata == other.recipient_metadata
            && self.canonincal_identifier == other.canonincal_identifier
    }
}

#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SendWithdrawExpired {
    #[deref]
    pub inner: SendMessageEventInner,
    pub participant: Address,
    pub nonce: U256,
    pub expiration: U64,
}

#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SendWithdrawRequest {
    #[deref]
    pub inner: SendMessageEventInner,
    pub participant: Address,
    pub expiration: U64,
    pub nonce: U256,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ContractSendEvent {
    pub triggered_by_blockhash: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ContractSendChannelSettle {
    pub inner: ContractSendEvent,
    pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ContractSendChannelUpdateTransfer {
    pub inner: ContractSendEvent,
    pub expiration: U256,
    pub balance_proof: BalanceProofState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ContractSendChannelBatchUnlock {
    pub inner: ContractSendEvent,
    pub canonical_identifier: CanonicalIdentifier,
    pub sender: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct EventInvalidActionWithdraw {
    pub attemped_withdraw: U256,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct EventInvalidActionSetRevealTimeout {
    pub reveal_timeout: U64,
    pub reason: String,
}
