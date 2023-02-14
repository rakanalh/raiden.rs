use raiden_blockchain::keys::{
	signature_to_bytes,
	PrivateKey,
};
use raiden_state_machine::types::{
	AddressMetadata,
	ChainID,
	MessageIdentifier,
	QueueIdentifier,
	SendWithdrawExpired,
	TokenNetworkAddress,
	U64,
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
		U256,
	},
};

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
	WithdrawExpired(WithdrawExpired),
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

#[derive(Clone, Serialize, Deserialize)]
pub struct WithdrawExpired {
	message_identifier: u32,
	chain_id: ChainID,
	token_network_address: TokenNetworkAddress,
	channel_identifier: U256,
	participant: Address,
	total_withdraw: U256,
	expiration: U64,
	nonce: U256,
	signature: Vec<u8>,
}

impl From<SendWithdrawExpired> for WithdrawExpired {
	fn from(event: SendWithdrawExpired) -> Self {
		Self {
			message_identifier: event.message_identifier,
			chain_id: event.canonical_identifier.chain_identifier.clone(),
			token_network_address: event.canonical_identifier.token_network_address,
			channel_identifier: event.canonical_identifier.channel_identifier,
			participant: event.participant,
			total_withdraw: event.total_withdraw,
			expiration: event.expiration,
			nonce: event.nonce,
			signature: vec![],
		}
	}
}

impl SignedMessage for WithdrawExpired {
	fn bytes(&self) -> Vec<u8> {
		let chain_id: Vec<u8> = self.chain_id.into();
		let cmd_id: [u8; 1] = CmdId::WithdrawExpired.into();
		let message_type_id: [u8; 1] = MessageTypeId::Withdraw.into();

		let mut nonce = [0u8; 32];
		self.nonce.to_big_endian(&mut nonce);

		let mut channel_identifier = [0u8; 32];
		self.channel_identifier.to_big_endian(&mut channel_identifier);

		let mut total_withdraw = [0u8; 32];
		self.total_withdraw.to_big_endian(&mut total_withdraw);

		let mut expiration = [0u8; 32];
		self.expiration.to_big_endian(&mut expiration);

		let mut bytes = vec![];
		bytes.extend_from_slice(&cmd_id);
		bytes.extend_from_slice(&nonce);
		bytes.extend_from_slice(&self.message_identifier.to_be_bytes());
		bytes.extend_from_slice(self.token_network_address.as_bytes());
		bytes.extend_from_slice(&chain_id);
		bytes.extend_from_slice(&message_type_id);
		bytes.extend_from_slice(&channel_identifier);
		bytes.extend_from_slice(self.participant.as_bytes());
		bytes.extend_from_slice(&total_withdraw);
		bytes.extend_from_slice(&expiration);
		bytes
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = signature_to_bytes(self.sign_message(key)?);
		Ok(())
	}
}
