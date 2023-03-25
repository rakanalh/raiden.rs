use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::{
	deserializers::{
		signature_from_str,
		u256_from_str,
		u64_from_str,
	},
	serializers::u256_to_str,
	traits::ToBytes,
	types::{
		Address,
		BlockExpiration,
		ChainID,
		MessageIdentifier,
		MessageTypeId,
		Signature,
		TokenNetworkAddress,
		U256,
	},
};
use raiden_state_machine::types::{
	SendWithdrawConfirmation,
	SendWithdrawExpired,
	SendWithdrawRequest,
};
use serde::{
	Deserialize,
	Serialize,
};
use web3::signing::SigningError;

use super::{
	CmdId,
	SignedMessage,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct WithdrawRequest {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
	pub message_identifier: MessageIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub channel_identifier: U256,
	pub participant: Address,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub total_withdraw: U256,
	pub expiration: BlockExpiration,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub nonce: U256,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
	pub coop_settle: bool,
}

impl From<SendWithdrawRequest> for WithdrawRequest {
	fn from(event: SendWithdrawRequest) -> Self {
		Self {
			message_identifier: event.message_identifier,
			chain_id: event.canonical_identifier.chain_identifier.clone(),
			token_network_address: event.canonical_identifier.token_network_address,
			channel_identifier: event.canonical_identifier.channel_identifier,
			participant: event.participant,
			total_withdraw: event.total_withdraw,
			expiration: event.expiration,
			nonce: event.nonce,
			signature: Signature::default(),
			coop_settle: event.coop_settle,
		}
	}
}

impl SignedMessage for WithdrawRequest {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let chain_id: Vec<u8> = self.chain_id.into();

		let mut nonce = [0u8; 32];
		self.nonce.to_big_endian(&mut nonce);

		let mut channel_identifier = [0u8; 32];
		self.channel_identifier.to_big_endian(&mut channel_identifier);

		let mut total_withdraw = [0u8; 32];
		self.total_withdraw.to_big_endian(&mut total_withdraw);

		let expiration = self.expiration.to_be_bytes();

		let mut bytes = vec![];
		bytes.extend_from_slice(self.token_network_address.as_bytes());
		bytes.extend_from_slice(&chain_id);
		bytes.extend_from_slice(&channel_identifier);
		bytes.extend_from_slice(self.participant.as_bytes());
		bytes.extend_from_slice(&total_withdraw);
		bytes.extend_from_slice(&expiration);
		bytes
	}

	fn bytes_to_pack(&self) -> Vec<u8> {
		vec![]
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct WithdrawConfirmation {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
	pub message_identifier: MessageIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub channel_identifier: U256,
	pub participant: Address,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub total_withdraw: U256,
	pub expiration: BlockExpiration,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub nonce: U256,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
}

impl From<SendWithdrawConfirmation> for WithdrawConfirmation {
	fn from(event: SendWithdrawConfirmation) -> Self {
		Self {
			message_identifier: event.message_identifier,
			chain_id: event.canonical_identifier.chain_identifier.clone(),
			token_network_address: event.canonical_identifier.token_network_address,
			channel_identifier: event.canonical_identifier.channel_identifier,
			participant: event.participant,
			total_withdraw: event.total_withdraw,
			expiration: event.expiration,
			nonce: event.nonce,
			signature: Signature::default(),
		}
	}
}

impl SignedMessage for WithdrawConfirmation {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let chain_id: Vec<u8> = self.chain_id.into();

		let mut nonce = [0u8; 32];
		self.nonce.to_big_endian(&mut nonce);

		let mut channel_identifier = [0u8; 32];
		self.channel_identifier.to_big_endian(&mut channel_identifier);

		let mut total_withdraw = [0u8; 32];
		self.total_withdraw.to_big_endian(&mut total_withdraw);

		let expiration = self.expiration.to_be_bytes();

		let mut bytes = vec![];
		bytes.extend_from_slice(self.token_network_address.as_bytes());
		bytes.extend_from_slice(&chain_id);
		bytes.extend_from_slice(&channel_identifier);
		bytes.extend_from_slice(self.participant.as_bytes());
		bytes.extend_from_slice(&total_withdraw);
		bytes.extend_from_slice(&expiration);
		bytes
	}

	fn bytes_to_pack(&self) -> Vec<u8> {
		vec![]
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct WithdrawExpired {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
	pub message_identifier: MessageIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub channel_identifier: U256,
	pub participant: Address,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub total_withdraw: U256,
	pub expiration: BlockExpiration,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub nonce: U256,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
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
			signature: Signature::default(),
		}
	}
}

impl SignedMessage for WithdrawExpired {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let chain_id: Vec<u8> = self.chain_id.into();
		let message_type_id: [u8; 1] = MessageTypeId::Withdraw.into();

		let mut nonce = [0u8; 32];
		self.nonce.to_big_endian(&mut nonce);

		let mut channel_identifier = [0u8; 32];
		self.channel_identifier.to_big_endian(&mut channel_identifier);

		let mut total_withdraw = [0u8; 32];
		self.total_withdraw.to_big_endian(&mut total_withdraw);

		let expiration = self.expiration.to_be_bytes();

		let mut bytes = vec![];
		bytes.extend(&[CmdId::WithdrawExpired as u8, 0, 0, 0]);
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

	fn bytes_to_pack(&self) -> Vec<u8> {
		vec![]
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}
