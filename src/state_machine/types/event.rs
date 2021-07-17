use std::ops::Deref;

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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Event {
    SendWithdrawExpired(SendWithdrawExpired),
    SendWithdrawRequest(SendWithdrawRequest),
    ContractSendChannelSettle(ContractSendChannelSettle),
    ContractSendChannelUpdateTransfer(ContractSendChannelUpdateTransfer),
    ContractSendChannelBatchUnlock(ContractSendChannelBatchUnlock),
    InvalidActionWithdraw(EventInvalidActionWithdraw),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SendMessageEvent {
    SendWithdrawExpired(SendWithdrawExpired),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SendMessageEventInner {
    pub recipient: Address,
    pub recipient_metadata: Option<AddressMetadata>,
    pub canonincal_identifier: CanonicalIdentifier,
    pub message_identifier: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendWithdrawExpired {
    pub inner: SendMessageEventInner,
    pub participant: Address,
    pub nonce: U256,
    pub expiration: U64,
}

impl Deref for SendWithdrawExpired {
    type Target = SendMessageEventInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendWithdrawRequest {
    pub inner: SendMessageEventInner,
    pub participant: Address,
    pub expiration: U64,
    pub nonce: U256,
}

impl Deref for SendWithdrawRequest {
    type Target = SendMessageEventInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractSendEvent {
    pub triggered_by_blockhash: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractSendChannelSettle {
    pub inner: ContractSendEvent,
    pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractSendChannelUpdateTransfer {
    pub inner: ContractSendEvent,
    pub expiration: U256,
    pub balance_proof: BalanceProofState,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractSendChannelBatchUnlock {
    pub inner: ContractSendEvent,
    pub canonical_identifier: CanonicalIdentifier,
    pub sender: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventInvalidActionWithdraw {
    pub attemped_withdraw: U256,
    pub reason: String,
}
