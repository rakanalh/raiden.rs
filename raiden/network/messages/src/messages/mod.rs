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

impl Into<[u8; 1]> for CmdId {
	fn into(self) -> [u8; 1] {
		(self as u8).to_be_bytes()
	}
}

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutgoingMessage {
	pub message_identifier: MessageIdentifier,
	pub recipient: Address,
	pub recipient_metadata: AddressMetadata,
	#[serde(flatten)]
	pub inner: MessageInner,
}

impl OutgoingMessage {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IncomingMessage {
	pub message_identifier: MessageIdentifier,
	pub inner: MessageInner,
}

impl IncomingMessage {
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

pub trait SignedMessage {
	fn bytes_to_sign(&self) -> Vec<u8>;
	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError>;
	fn sign_message(&self, key: PrivateKey) -> Result<Signature, SigningError> {
		let bytes = self.bytes_to_sign();
		key.sign_message(&bytes)
	}
}

pub trait SignedEnvelopeMessage: SignedMessage {
	fn message_hash(&self) -> H256;
}

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
