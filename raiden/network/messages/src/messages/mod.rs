use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::types::{
	Address,
	AddressMetadata,
	BlockNumber,
	MessageIdentifier,
	QueueIdentifier,
	H256,
	U256,
};
use serde::{
	Deserialize,
	Serialize,
};
use web3::signing::{
	Key,
	Signature,
	SigningError,
};

mod metadata;
mod monitoring_service;
mod pathfinding;
mod synchronization;
mod transfer;
mod withdraw;

pub use metadata::*;
pub use monitoring_service::*;
pub use pathfinding::*;
pub use synchronization::*;
pub use transfer::*;
pub use withdraw::*;

/// Identifier for off-chain messages.
///
/// These magic numbers are used to identify the type of a message.
enum CmdId {
	Processed = 0,
	SecretRequest = 3,
	Unlock = 4,
	LockedTransfer = 7,
	RevealSecret = 11,
	Delivered = 12,
	LockExpired = 13,
	WithdrawExpired = 17,
}

impl From<CmdId> for [u8; 1] {
	fn from(val: CmdId) -> Self {
		(val as u8).to_be_bytes()
	}
}

/// An enum containing the commands to send to the transport layer.
#[derive(Debug, Eq, PartialEq)]
pub enum TransportServiceMessage {
	Enqueue((QueueIdentifier, OutgoingMessage)),
	Dequeue((Option<QueueIdentifier>, MessageIdentifier)),
	Send(MessageIdentifier),
	Broadcast(OutgoingMessage),
	UpdateServiceAddresses(Address, U256),
	ExpireServiceAddresses(U256, BlockNumber),
	Clear(QueueIdentifier),
}

/// An enum containing all message types to be sent / received by the transport.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageInner {
	LockedTransfer(LockedTransfer),
	LockExpired(LockExpired),
	SecretRequest(SecretRequest),
	SecretReveal(SecretReveal),
	Unlock(Unlock),
	WithdrawRequest(WithdrawRequest),
	WithdrawConfirmation(WithdrawConfirmation),
	WithdrawExpired(WithdrawExpired),
	PFSCapacityUpdate(PFSCapacityUpdate),
	PFSFeeUpdate(PFSFeeUpdate),
	MSUpdate(RequestMonitoring),
	Processed(Processed),
	Delivered(Delivered),
}

/// Message to be sent out to the partner node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutgoingMessage {
	pub message_identifier: MessageIdentifier,
	pub recipient: Address,
	pub recipient_metadata: AddressMetadata,
	#[serde(flatten)]
	pub inner: MessageInner,
}

impl OutgoingMessage {
	/// Returns the string type name of the message.
	pub fn type_name(&self) -> &'static str {
		match self.inner {
			MessageInner::LockedTransfer(_) => "LockedTransfer",
			MessageInner::LockExpired(_) => "LockExpired",
			MessageInner::SecretRequest(_) => "SecretRequest",
			MessageInner::SecretReveal(_) => "SecretReveal",
			MessageInner::Unlock(_) => "Unlock",
			MessageInner::WithdrawRequest(_) => "WithdrawRequest",
			MessageInner::WithdrawConfirmation(_) => "WithdrawConfirmation",
			MessageInner::WithdrawExpired(_) => "WithdrawExpired",
			MessageInner::PFSCapacityUpdate(_) => "PFSCapacityUpdate",
			MessageInner::PFSFeeUpdate(_) => "PFSFeeUpdate",
			MessageInner::MSUpdate(_) => "MSUpdate",
			MessageInner::Processed(_) => "Processed",
			MessageInner::Delivered(_) => "Delivered",
		}
	}
}

/// Message received from the partner node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IncomingMessage {
	pub message_identifier: MessageIdentifier,
	pub inner: MessageInner,
}

impl IncomingMessage {
	/// Returns the string type name of the message.
	pub fn type_name(&self) -> &'static str {
		match self.inner {
			MessageInner::LockedTransfer(_) => "LockedTransfer",
			MessageInner::LockExpired(_) => "LockExpired",
			MessageInner::SecretRequest(_) => "SecretRequest",
			MessageInner::SecretReveal(_) => "SecretReveal",
			MessageInner::Unlock(_) => "Unlock",
			MessageInner::WithdrawRequest(_) => "WithdrawRequest",
			MessageInner::WithdrawConfirmation(_) => "WithdrawConfirmation",
			MessageInner::WithdrawExpired(_) => "WithdrawExpired",
			MessageInner::PFSCapacityUpdate(_) => "PFSCapacityUpdate",
			MessageInner::PFSFeeUpdate(_) => "PFSFeeUpdate",
			MessageInner::MSUpdate(_) => "MSUpdate",
			MessageInner::Processed(_) => "Processed",
			MessageInner::Delivered(_) => "Delivered",
		}
	}
}

/// Trait to be implemented by the messages that have to be signed before being sent.
pub trait SignedMessage {
	fn bytes_to_sign(&self) -> Vec<u8>;
	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError>;
	fn sign_message(&self, key: PrivateKey) -> Result<Signature, SigningError> {
		let bytes = self.bytes_to_sign();
		key.sign_message(&bytes)
	}
}

/// A trait for the signed message that contains a balance proof.
pub trait SignedEnvelopeMessage: SignedMessage {
	fn message_hash(&self) -> H256;
}

/// Convert state machine event into a signed message.
#[macro_export]
macro_rules! to_message {
	( $send_message_event:ident, $private_key:ident, $message_type:tt ) => {{
		let message_identifier = $send_message_event.inner.message_identifier;
		let recipient = $send_message_event.inner.recipient;
		let address_metadata = $send_message_event
			.inner
			.recipient_metadata
			.clone()
			.expect("Address metadata should be set at this point");
		let mut message: $message_type = $send_message_event.into();
		let _ = message.sign($private_key);
		OutgoingMessage {
			message_identifier,
			recipient,
			recipient_metadata: address_metadata,
			inner: MessageInner::$message_type(message),
		}
	}};
}
