use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::{
	deserializers::u64_from_str,
	traits::ToBytes,
	types::{
		MessageIdentifier,
		Signature,
	},
};
use raiden_state_machine::types::SendProcessed;
use serde::{
	Deserialize,
	Serialize,
};
use web3::signing::SigningError;

use super::{
	CmdId,
	SignedMessage,
};

/// Used by the recipient when a message which has to be validated against
/// blockchain data was successfully processed.
///
/// This message is only used to confirm the processing of messages which have
/// some blockchain related data, where receiving the message is not
/// sufficient. Consider the following scenario:
///
/// - Node A starts a deposit of 5 tokens.
/// - Node A sees the deposit, and starts a transfer.
/// - Node B receives the transfer, however it has not seen the deposit, therefore the transfer is
///   rejected.
///
/// Second scenario:
///
/// - Node A has a lock which has expired, and sends the RemoveExpiredLock message.
/// - Node B receives the message, but from its perspective the block at which the lock expires has
///   not been confirmed yet, meaning that a reorg is possible and the secret can be registered
///   on-chain.
///
/// For both scenarios A has to keep retrying until B accepts the message.
///
/// Notes:
///     - This message is required even if the transport guarantees durability of the data.
///     - This message provides a stronger guarantee then a Delivered, therefore it can replace it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct Processed {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
	pub message_identifier: MessageIdentifier,
	pub signature: Signature,
}

impl From<SendProcessed> for Processed {
	fn from(event: SendProcessed) -> Self {
		Self { message_identifier: event.message_identifier, signature: Signature::default() }
	}
}

impl SignedMessage for Processed {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let cmd_id: [u8; 1] = CmdId::Processed.into();

		let mut bytes = vec![];
		bytes.extend_from_slice(&cmd_id);
		bytes.extend_from_slice(&[0, 0, 0]);
		bytes.extend_from_slice(&self.message_identifier.to_be_bytes());
		bytes
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

/// Informs the sender that the message was received *and* persisted.
///
/// Notes:
///     - This message provides a weaker guarantee in respect to the Processed message. It can be
///       emulated by a transport layer that guarantees persistence, or it can be sent by the
///       recipient before the received message is processed (therefore it does not matter if the
///       message was successfully processed or not).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct Delivered {
	#[serde(deserialize_with = "u64_from_str")]
	pub delivered_message_identifier: MessageIdentifier,
	pub signature: Signature,
}

impl SignedMessage for Delivered {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let cmd_id: [u8; 1] = CmdId::Delivered.into();

		let mut bytes = vec![];
		bytes.extend_from_slice(&cmd_id);
		bytes.extend_from_slice(&[0, 0, 0]);
		bytes.extend_from_slice(&self.delivered_message_identifier.to_be_bytes());
		bytes
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}
