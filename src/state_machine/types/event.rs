use derive_more::Deref;
use serde::{
    Deserialize,
    Serialize,
};
use web3::types::Address;

use crate::primitives::{
    AddressMetadata,
    BlockExpiration,
    BlockHash,
    CanonicalIdentifier,
    MessageIdentifier,
    Nonce,
    PaymentIdentifier,
    QueueIdentifier,
    RevealTimeout,
    Secret,
    SecretHash,
    TokenAmount,
    TokenNetworkAddress,
    TokenNetworkRegistryAddress,
};

use super::{
    BalanceProofState,
    LockedTransferState,
};

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum Event {
    SendWithdrawExpired(SendWithdrawExpired),
    SendWithdrawRequest(SendWithdrawRequest),
    SendLockedTransfer(SendLockedTransfer),
    ContractSendChannelSettle(ContractSendChannelSettle),
    ContractSendChannelUpdateTransfer(ContractSendChannelUpdateTransfer),
    ContractSendChannelBatchUnlock(ContractSendChannelBatchUnlock),
    InvalidActionWithdraw(EventInvalidActionWithdraw),
    InvalidActionSetRevealTimeout(EventInvalidActionSetRevealTimeout),
    PaymentSentFailed(EventPaymentSentFailed),
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum SendMessageEvent {
    SendWithdrawExpired(SendWithdrawExpired),
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
#[cfg_attr(not(test), derive(PartialEq))]
pub struct SendMessageEventInner {
    pub recipient: Address,
    pub recipient_metadata: Option<AddressMetadata>,
    pub canonical_identifier: CanonicalIdentifier,
    pub message_identifier: MessageIdentifier,
}

impl SendMessageEventInner {
    pub fn queue_identifier(&self) -> QueueIdentifier {
        QueueIdentifier {
            recipient: self.recipient.clone(),
            canonical_identifier: self.canonical_identifier.clone(),
        }
    }
}

#[cfg(test)]
impl PartialEq for SendMessageEventInner {
    fn eq(&self, other: &Self) -> bool {
        self.recipient == other.recipient
            && self.recipient_metadata == other.recipient_metadata
            && self.canonical_identifier == other.canonical_identifier
    }
}

#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SendWithdrawExpired {
    #[deref]
    pub inner: SendMessageEventInner,
    pub participant: Address,
    pub total_withdraw: TokenAmount,
    pub nonce: Nonce,
    pub expiration: BlockExpiration,
}

#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SendWithdrawRequest {
    #[deref]
    pub inner: SendMessageEventInner,
    pub participant: Address,
    pub expiration: BlockExpiration,
    pub nonce: Nonce,
}

#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SendLockedTransfer {
    #[deref]
    pub inner: SendMessageEventInner,
    pub transfer: LockedTransferState,
}

#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SendSecretReveal {
    #[deref]
    pub inner: SendMessageEventInner,
    pub secret: Secret,
    pub secrethash: SecretHash,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ContractSendEvent {
    pub triggered_by_blockhash: BlockHash,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ContractSendChannelSettle {
    pub inner: ContractSendEvent,
    pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ContractSendChannelUpdateTransfer {
    pub inner: ContractSendEvent,
    pub expiration: BlockExpiration,
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
    pub attemped_withdraw: TokenAmount,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct EventInvalidActionSetRevealTimeout {
    pub reveal_timeout: RevealTimeout,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct EventPaymentSentFailed {
    pub token_network_registry_address: TokenNetworkRegistryAddress,
    pub token_network_address: TokenNetworkAddress,
    pub identifier: PaymentIdentifier,
    pub target: Address,
    pub reason: String,
}
