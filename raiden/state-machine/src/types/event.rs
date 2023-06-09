#![warn(clippy::missing_docs_in_private_items)]

use derive_more::Deref;
use raiden_macros::IntoEvent;
use raiden_primitives::types::{
	Address,
	AddressMetadata,
	BlockExpiration,
	BlockHash,
	BlockNumber,
	CanonicalIdentifier,
	MessageIdentifier,
	Nonce,
	PaymentIdentifier,
	QueueIdentifier,
	RevealTimeout,
	Secret,
	SecretHash,
	Signature,
	TokenAddress,
	TokenAmount,
	TokenNetworkAddress,
	TokenNetworkRegistryAddress,
	U256,
};
use serde::{
	Deserialize,
	Serialize,
};

use super::{
	BalanceProofState,
	LockedTransferState,
	PFSUpdate,
};

/// An enum containing all possible event variants.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum Event {
	ContractSendChannelClose(ContractSendChannelClose),
	ContractSendChannelCoopSettle(ContractSendChannelCoopSettle),
	ContractSendChannelWithdraw(ContractSendChannelWithdraw),
	ContractSendChannelSettle(ContractSendChannelSettle),
	ContractSendChannelUpdateTransfer(ContractSendChannelUpdateTransfer),
	ContractSendChannelBatchUnlock(ContractSendChannelBatchUnlock),
	ContractSendSecretReveal(ContractSendSecretReveal),
	PaymentReceivedSuccess(PaymentReceivedSuccess),
	PaymentSentSuccess(PaymentSentSuccess),
	SendWithdrawExpired(SendWithdrawExpired),
	SendWithdrawRequest(SendWithdrawRequest),
	SendWithdrawConfirmation(SendWithdrawConfirmation),
	SendLockedTransfer(SendLockedTransfer),
	SendLockExpired(SendLockExpired),
	SendSecretRequest(SendSecretRequest),
	SendSecretReveal(SendSecretReveal),
	SendUnlock(SendUnlock),
	SendPFSUpdate(PFSUpdate),
	SendMSUpdate(BalanceProofState),
	SendProcessed(SendProcessed),
	UnlockSuccess(UnlockSuccess),
	UnlockClaimSuccess(UnlockClaimSuccess),
	UpdatedServicesAddresses(UpdatedServicesAddresses),
	ExpireServicesAddresses(BlockNumber),
	ErrorInvalidActionWithdraw(ErrorInvalidActionWithdraw),
	ErrorInvalidActionCoopSettle(ErrorInvalidActionCoopSettle),
	ErrorInvalidActionSetRevealTimeout(ErrorInvalidActionSetRevealTimeout),
	ErrorInvalidSecretRequest(ErrorInvalidSecretRequest),
	ErrorInvalidReceivedLockedTransfer(ErrorInvalidReceivedLockedTransfer),
	ErrorInvalidReceivedLockExpired(ErrorInvalidReceivedLockExpired),
	ErrorInvalidReceivedTransferRefund(ErrorInvalidReceivedTransferRefund),
	ErrorInvalidReceivedUnlock(ErrorInvalidReceivedUnlock),
	ErrorInvalidReceivedWithdrawRequest(ErrorInvalidReceivedWithdrawRequest),
	ErrorInvalidReceivedWithdrawConfirmation(ErrorInvalidReceivedWithdrawConfirmation),
	ErrorInvalidReceivedWithdrawExpired(ErrorInvalidReceivedWithdrawExpired),
	ErrorPaymentSentFailed(ErrorPaymentSentFailed),
	ErrorRouteFailed(ErrorRouteFailed),
	ErrorUnlockClaimFailed(ErrorUnlockClaimFailed),
	ErrorUnlockFailed(ErrorUnlockFailed),
	ErrorUnexpectedReveal(ErrorUnexpectedReveal),
	ClearMessages(QueueIdentifier),
}

impl Event {
	/// Returns a string of the inner event's type name.
	pub fn type_name(&self) -> &'static str {
		match self {
			Event::ContractSendChannelClose(_) => "ContractSendChannelClose",
			Event::ContractSendChannelCoopSettle(_) => "ContractSendChannelCoopSettle",
			Event::ContractSendChannelWithdraw(_) => "ContractSendChannelWithdraw",
			Event::ContractSendChannelSettle(_) => "ContractSendChannelSettle",
			Event::ContractSendChannelUpdateTransfer(_) => "ContractSendChannelUpdateTransfer",
			Event::ContractSendChannelBatchUnlock(_) => "ContractSendChannelBatchUnlock",
			Event::ContractSendSecretReveal(_) => "ContractSendSecretReveal",
			Event::PaymentReceivedSuccess(_) => "PaymentReceivedSuccess",
			Event::PaymentSentSuccess(_) => "PaymentSentSuccess",
			Event::SendWithdrawExpired(_) => "SendWithdrawExpired",
			Event::SendWithdrawRequest(_) => "SendWithdrawRequest",
			Event::SendWithdrawConfirmation(_) => "SendWithdrawConfirmation",
			Event::SendLockedTransfer(_) => "SendLockedTransfer",
			Event::SendLockExpired(_) => "SendLockExpired",
			Event::SendSecretRequest(_) => "SendSecretRequest",
			Event::SendSecretReveal(_) => "SendSecretReveal",
			Event::SendUnlock(_) => "SendUnlock",
			Event::SendPFSUpdate(_) => "SendPFSUpdate",
			Event::SendMSUpdate(_) => "SendMSUpdate",
			Event::SendProcessed(_) => "SendProcessed",
			Event::UnlockSuccess(_) => "UnlockSuccess",
			Event::UnlockClaimSuccess(_) => "UnlockClaimSuccess",
			Event::UpdatedServicesAddresses(_) => "UpdatedServicesAddresses",
			Event::ExpireServicesAddresses(_) => "ExpireServicesAddresses",
			Event::ErrorInvalidActionWithdraw(_) => "ErrorInvalidActionWithdraw",
			Event::ErrorInvalidActionCoopSettle(_) => "ErrorInvalidActionCoopSettle",
			Event::ErrorInvalidActionSetRevealTimeout(_) => "ErrorInvalidActionSetRevealTimeout",
			Event::ErrorInvalidSecretRequest(_) => "ErrorInvalidSecretRequest",
			Event::ErrorInvalidReceivedLockedTransfer(_) => "ErrorInvalidReceivedLockedTransfer",
			Event::ErrorInvalidReceivedLockExpired(_) => "ErrorInvalidReceivedLockExpired",
			Event::ErrorInvalidReceivedTransferRefund(_) => "ErrorInvalidReceivedTransferRefund",
			Event::ErrorInvalidReceivedUnlock(_) => "ErrorInvalidReceivedUnlock",
			Event::ErrorInvalidReceivedWithdrawRequest(_) => "ErrorInvalidReceivedWithdrawRequest",
			Event::ErrorInvalidReceivedWithdrawConfirmation(_) =>
				"ErrorInvalidReceivedWithdrawConfirmation",
			Event::ErrorInvalidReceivedWithdrawExpired(_) => "ErrorInvalidReceivedWithdrawExpired",
			Event::ErrorPaymentSentFailed(_) => "ErrorPaymentSentFailed",
			Event::ErrorRouteFailed(_) => "ErrorRouteFailed",
			Event::ErrorUnlockClaimFailed(_) => "ErrorUnlockClaimFailed",
			Event::ErrorUnlockFailed(_) => "ErrorUnlockFailed",
			Event::ErrorUnexpectedReveal(_) => "ErrorUnexpectedReveal",
			Event::ClearMessages(_) => "ClearMessages",
		}
	}
}

/// An enum of the SendEvent variants.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum SendMessageEvent {
	SendLockExpired(SendLockExpired),
	SendLockedTransfer(SendLockedTransfer),
	SendSecretReveal(SendSecretReveal),
	SendSecretRequest(SendSecretRequest),
	SendUnlock(SendUnlock),
	SendWithdrawRequest(SendWithdrawRequest),
	SendWithdrawConfirmation(SendWithdrawConfirmation),
	SendWithdrawExpired(SendWithdrawExpired),
	SendProcessed(SendProcessed),
}

impl TryFrom<Event> for SendMessageEvent {
	type Error = ();

	fn try_from(event: Event) -> Result<Self, Self::Error> {
		Ok(match event {
			Event::SendWithdrawExpired(inner) => SendMessageEvent::SendWithdrawExpired(inner),
			Event::SendWithdrawRequest(inner) => SendMessageEvent::SendWithdrawRequest(inner),
			Event::SendLockedTransfer(inner) => SendMessageEvent::SendLockedTransfer(inner),
			Event::SendLockExpired(inner) => SendMessageEvent::SendLockExpired(inner),
			Event::SendSecretRequest(inner) => SendMessageEvent::SendSecretRequest(inner),
			Event::SendSecretReveal(inner) => SendMessageEvent::SendSecretReveal(inner),
			Event::SendUnlock(inner) => SendMessageEvent::SendUnlock(inner),
			Event::SendProcessed(inner) => SendMessageEvent::SendProcessed(inner),
			_ => return Err(()),
		})
	}
}

/// An enum of the ContractSendEvent variants.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum ContractSendEvent {
	ContractSendChannelClose(ContractSendChannelClose),
	ContractSendChannelWithdraw(ContractSendChannelWithdraw),
	ContractSendChannelSettle(ContractSendChannelSettle),
	ContractSendChannelUpdateTransfer(ContractSendChannelUpdateTransfer),
	ContractSendChannelBatchUnlock(ContractSendChannelBatchUnlock),
	ContractSendSecretReveal(ContractSendSecretReveal),
}

impl TryFrom<Event> for ContractSendEvent {
	type Error = ();

	fn try_from(event: Event) -> Result<Self, Self::Error> {
		Ok(match event {
			Event::ContractSendChannelClose(inner) =>
				ContractSendEvent::ContractSendChannelClose(inner),
			Event::ContractSendChannelWithdraw(inner) =>
				ContractSendEvent::ContractSendChannelWithdraw(inner),
			Event::ContractSendChannelSettle(inner) =>
				ContractSendEvent::ContractSendChannelSettle(inner),
			Event::ContractSendChannelUpdateTransfer(inner) =>
				ContractSendEvent::ContractSendChannelUpdateTransfer(inner),
			Event::ContractSendChannelBatchUnlock(inner) =>
				ContractSendEvent::ContractSendChannelBatchUnlock(inner),
			Event::ContractSendSecretReveal(inner) =>
				ContractSendEvent::ContractSendSecretReveal(inner),
			_ => return Err(()),
		})
	}
}

/// Common message attributes.
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
			recipient: self.recipient,
			canonical_identifier: self.canonical_identifier.clone(),
		}
	}
}

#[cfg(test)]
impl PartialEq for SendMessageEventInner {
	fn eq(&self, other: &Self) -> bool {
		self.recipient == other.recipient &&
			self.recipient_metadata == other.recipient_metadata &&
			self.canonical_identifier == other.canonical_identifier
	}
}

/// Event used by node to request a withdraw from channel partner.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct SendWithdrawRequest {
	#[deref]
	#[serde(flatten)]
	pub inner: SendMessageEventInner,
	pub total_withdraw: TokenAmount,
	pub participant: Address,
	pub expiration: BlockExpiration,
	pub nonce: Nonce,
	pub coop_settle: bool,
}

/// Event used by node to confirm a withdraw for a channel's partner.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct SendWithdrawConfirmation {
	#[deref]
	pub inner: SendMessageEventInner,
	pub participant: Address,
	pub total_withdraw: TokenAmount,
	pub nonce: Nonce,
	pub expiration: BlockExpiration,
}

/// Event used by node to expire a withdraw request.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct SendWithdrawExpired {
	#[deref]
	pub inner: SendMessageEventInner,
	pub participant: Address,
	pub total_withdraw: TokenAmount,
	pub nonce: Nonce,
	pub expiration: BlockExpiration,
}

/// A locked transfer that must be sent to `recipient`.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct SendLockedTransfer {
	#[deref]
	pub inner: SendMessageEventInner,
	pub transfer: LockedTransferState,
}

/// Event used by a target node to request the secret from the initiator
/// (`recipient`).
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct SendSecretRequest {
	#[deref]
	pub inner: SendMessageEventInner,
	pub payment_identifier: PaymentIdentifier,
	pub amount: TokenAmount,
	pub expiration: BlockExpiration,
	pub secrethash: SecretHash,
}

/// Sends a SecretReveal to another node.
///
/// This event is used once the secret is known locally and an action must be
/// performed on the recipient:
///
/// - For receivers in the payee role, it informs the node that the lock has been released and the
///   token can be claimed, either on-chain or off-chain.
/// - For receivers in the payer role, it tells the payer that the payee knows the secret and wants
///   to claim the lock off-chain, so the payer may unlock the lock and send an up-to-date balance
///   proof to the payee, avoiding on-chain payments which would require the channel to be closed.
///
/// For any mediated transfer:
/// - The initiator will only perform the payer role.
/// - The target will only perform the payee role.
/// - The mediators will have `n` channels at the payee role and `n` at the payer role, where `n` is
///   equal to `1 + number_of_refunds`.
///
/// Note:
///   The payee must only update its local balance once the payer sends an
///   up-to-date balance-proof message. This is a requirement for keeping the
///   nodes synchronized. The reveal secret message flows from the recipient
///   to the sender, so when the secret is learned it is not yet time to
///   update the balance.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct SendSecretReveal {
	#[deref]
	pub inner: SendMessageEventInner,
	pub secret: Secret,
	pub secrethash: SecretHash,
}

/// Sends a LockExpired to another node.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct SendLockExpired {
	#[deref]
	pub inner: SendMessageEventInner,
	pub balance_proof: BalanceProofState,
	pub secrethash: SecretHash,
}

/// Event to send a balance-proof to the counter-party, used after a lock
/// is unlocked locally allowing the counter-party to claim it.
///
/// Used by payers: The initiator and mediator nodes.
///
/// Note:
/// This event has a dual role, it serves as a synchronization and as
/// balance-proof for the netting channel smart contract.
///
/// Nodes need to keep the last known locksroot synchronized. This is
/// required by the receiving end of a transfer in order to properly
/// validate. The rule is "only the party that owns the current payment
/// channel may change it" (remember that a netting channel is composed of
/// two uni-directional channels), as a consequence the locksroot is only
/// updated by the recipient once a balance proof message is received.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct SendUnlock {
	#[deref]
	pub inner: SendMessageEventInner,
	pub payment_identifier: PaymentIdentifier,
	pub token_address: TokenAddress,
	pub balance_proof: BalanceProofState,
	pub secret: Secret,
	pub secrethash: SecretHash,
}

/// Send a Processed to another node.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct SendProcessed {
	#[deref]
	pub inner: SendMessageEventInner,
}

/// Event emitted when a payee has received a payment.
///
/// Note:
///     A payee knows if a lock claim has failed, but this is not sufficient
///     information to deduce when a transfer has failed, because the initiator may
///     try again at a different time and/or with different routes, for this reason
///     there is no correspoding `EventTransferReceivedFailed`.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct PaymentReceivedSuccess {
	pub token_network_registry_address: TokenNetworkRegistryAddress,
	pub token_network_address: TokenNetworkAddress,
	pub identifier: PaymentIdentifier,
	pub amount: TokenAmount,
	pub initiator: Address,
}

/// Event emitted by the initiator when a transfer is considered successful.
///
/// A transfer is considered successful when the initiator's payee hop sends the
/// reveal secret message, assuming that each hop in the mediator chain has
/// also learned the secret and unlocked its token off-chain or on-chain.
///
/// This definition of successful is used to avoid the following corner case:
///
/// - The reveal secret message is sent, since the network is unreliable and we assume byzantine
///   behavior the message is considered delivered without an acknowledgement.
/// - The transfer is considered successful because of the above.
/// - The reveal secret message was not delivered because of actual network problems.
/// - The lock expires and an EventUnlockFailed follows, contradicting the EventPaymentSentSuccess.
///
/// Note:
///     Mediators cannot use this event, since an off-chain unlock may be locally
///     successful but there is no knowledge about the global transfer.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct PaymentSentSuccess {
	pub token_network_registry_address: TokenNetworkRegistryAddress,
	pub token_network_address: TokenNetworkAddress,
	pub identifier: PaymentIdentifier,
	pub amount: TokenAmount,
	pub target: Address,
	pub secret: Secret,
	pub route: Vec<Address>,
}

/// Event emitted when a lock unlock succeded.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct UnlockSuccess {
	pub identifier: PaymentIdentifier,
	pub secrethash: SecretHash,
}

/// Event emitted when a lock claim succeded.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct UnlockClaimSuccess {
	pub identifier: PaymentIdentifier,
	pub secrethash: SecretHash,
}

/// Common attributes of events which represent on-chain transactions.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ContractSendEventInner {
	pub triggered_by_blockhash: BlockHash,
}

/// Event emitted to close the netting channel.
/// This event is used when a node needs to prepare the channel to unlock
/// on-chain.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ContractSendChannelClose {
	#[deref]
	pub inner: ContractSendEventInner,
	pub canonical_identifier: CanonicalIdentifier,
	pub balance_proof: Option<BalanceProofState>,
}

/// Event emitted if node wants to cooperatively settle a channel.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ContractSendChannelCoopSettle {
	#[deref]
	pub inner: ContractSendEventInner,
	pub canonical_identifier: CanonicalIdentifier,
	pub our_total_withdraw: TokenAmount,
	pub partner_total_withdraw: TokenAmount,
	pub expiration: BlockExpiration,
	pub signature_our_withdraw: Signature,
	pub signature_partner_withdraw: Signature,
}

/// Event emitted if node wants to withdraw from current channel balance.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ContractSendChannelWithdraw {
	#[deref]
	pub inner: ContractSendEventInner,
	pub canonical_identifier: CanonicalIdentifier,
	pub total_withdraw: TokenAmount,
	pub expiration: BlockExpiration,
	pub partner_signature: Signature,
}

/// Event emitted if the netting channel must be settled.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ContractSendChannelSettle {
	#[deref]
	pub inner: ContractSendEventInner,
	pub canonical_identifier: CanonicalIdentifier,
}

/// Event emitted if the netting channel balance proof must be updated.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ContractSendChannelUpdateTransfer {
	#[deref]
	pub inner: ContractSendEventInner,
	pub expiration: BlockExpiration,
	pub balance_proof: BalanceProofState,
}

/// Look for unlocks that we should do after settlement
///
/// This will only lead to an on-chain unlock if there are locks that can be
/// unlocked to our benefit.
///
/// Usually, we would check if this is the case in the state machine and skip
/// the creation of this event if no profitable locks are found. But if a
/// channel was closed with another BP than the latest one, we need to look in
/// the database for the locks that correspond to the on-chain data. Searching
/// the database is not possible in the state machine, so we create this event
/// in every case and do the check in the event handler.
/// Since locks for both receiving and sending transfers can potentially return
/// tokens to use, this event leads to 0-2 on-chain transactions.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ContractSendChannelBatchUnlock {
	#[deref]
	pub inner: ContractSendEventInner,
	pub canonical_identifier: CanonicalIdentifier,
	pub sender: Address,
}

/// Event emitted when the lock must be claimed on-chain.
#[derive(Deref, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ContractSendSecretReveal {
	#[deref]
	pub inner: ContractSendEventInner,
	pub expiration: BlockExpiration,
	pub secret: Secret,
}

/// Event emitted when an invalid withdraw is initiated.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidActionWithdraw {
	pub attemped_withdraw: TokenAmount,
	pub reason: String,
}

/// Event emitted when an invalid withdraw request is received.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidReceivedWithdrawRequest {
	pub attemped_withdraw: TokenAmount,
	pub reason: String,
}

/// Event emitted when an invalid withdraw confirmation is received.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidReceivedWithdrawConfirmation {
	pub attemped_withdraw: TokenAmount,
	pub reason: String,
}

/// Event emitted when an invalid withdraw expired event is received.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidReceivedWithdrawExpired {
	pub attemped_withdraw: TokenAmount,
	pub reason: String,
}

/// Event emitted when an invalid withdraw is initiated.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidActionSetRevealTimeout {
	pub reveal_timeout: RevealTimeout,
	pub reason: String,
}

/// Event emitted by the payer when a transfer has failed.
///
/// Note:
///     Mediators cannot use this event since they don't know when a transfer
///     has failed, they may infer about lock successes and failures.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorPaymentSentFailed {
	pub token_network_registry_address: TokenNetworkRegistryAddress,
	pub token_network_address: TokenNetworkAddress,
	pub identifier: PaymentIdentifier,
	pub target: Address,
	pub reason: String,
}

/// Event emitted when a lock unlock failed.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorUnlockFailed {
	pub identifier: PaymentIdentifier,
	pub secrethash: SecretHash,
	pub reason: String,
}

/// Event emitted when a route failed.
/// As a payment can try different routes to reach the intended target
/// some of the routes can fail. This event is emitted when a route failed.
/// This means that multiple EventRouteFailed for a given payment and it's
/// therefore different to EventPaymentSentFailed.
/// A route can fail for two reasons:
/// - A refund transfer reaches the initiator (it's not important if this refund transfer is
///   unlocked or not)
/// - A lock expires
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorRouteFailed {
	pub secrethash: SecretHash,
	pub route: Vec<Address>,
	pub token_network_address: TokenNetworkAddress,
}

/// Event emitted when an invalid coop-settle is initiated.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidActionCoopSettle {
	pub attempted_withdraw: TokenAmount,
	pub reason: String,
}

/// Event emitted when an invalid SecretRequest is received.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidSecretRequest {
	pub payment_identifier: PaymentIdentifier,
	pub intended_amount: TokenAmount,
	pub actual_amount: TokenAmount,
}

/// Event emitted when an invalid locked transfer is received.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidReceivedLockedTransfer {
	pub payment_identifier: PaymentIdentifier,
	pub reason: String,
}

/// Event emitted when an invalid lock expired message is received.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidReceivedLockExpired {
	pub secrethash: SecretHash,
	pub reason: String,
}

/// Event emitted when an invalid refund transfer is received.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidReceivedTransferRefund {
	pub payment_identifier: PaymentIdentifier,
	pub reason: String,
}

/// Event emitted when an invalid unlock message is received.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorInvalidReceivedUnlock {
	pub secrethash: SecretHash,
	pub reason: String,
}

/// Event emitted when a lock claim failed.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorUnlockClaimFailed {
	pub identifier: PaymentIdentifier,
	pub secrethash: SecretHash,
	pub reason: String,
}

/// Event emitted when an unexpected secret reveal message is received.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct ErrorUnexpectedReveal {
	pub secrethash: SecretHash,
	pub reason: String,
}

/// Transition used when adding a new service address.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, IntoEvent)]
pub struct UpdatedServicesAddresses {
	pub service_address: Address,
	pub validity: U256,
}
