use raiden_blockchain::keys::PrivateKey;
use raiden_state_machine::types::{
	AddressMetadata,
	MessageIdentifier,
	QueueIdentifier,
};
use serde::{
	Deserialize,
	Serialize,
};
use web3::{
	signing::{
		Key,
		Signature,
		SigningError,
	},
	types::{
		Address,
		H256,
	},
};

mod metadata;
mod synchronization;
mod transfer;
mod withdraw;

pub use metadata::*;
pub use synchronization::*;
pub use transfer::*;
pub use withdraw::*;

#[allow(unused)]
enum CmdId {
	Processed = 0,
	Ping = 1,
	Pong = 2,
	SecretRequest = 3,
	Unlock = 4,
	LockedTransfer = 7,
	RefundTransfer = 8,
	RevealSecret = 11,
	Delivered = 12,
	LockExpired = 13,
	WithdrawRequest = 15,
	WithdrawConfirmation = 16,
	WithdrawExpired = 17,
}

impl Into<[u8; 1]> for CmdId {
	fn into(self) -> [u8; 1] {
		(self as u8).to_be_bytes()
	}
}

#[allow(unused)]
enum MessageTypeId {
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

pub enum TransportServiceMessage {
	Enqueue((QueueIdentifier, Message)),
	Send(Message),
}

#[derive(Clone, Serialize, Deserialize)]
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
	Processed(Processed),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
	pub message_identifier: MessageIdentifier,
	pub recipient: Address,
	pub recipient_metadata: AddressMetadata,
	pub inner: MessageInner,
}

pub trait SignedMessage {
	fn bytes(&self) -> Vec<u8>;
	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError>;
	fn sign_message(&self, key: PrivateKey) -> Result<Signature, SigningError> {
		let bytes = self.bytes();
		key.sign(&bytes, None)
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
		Message {
			message_identifier,
			recipient,
			recipient_metadata: address_metadata,
			inner: MessageInner::$message_type(message),
		}
	}};
}
