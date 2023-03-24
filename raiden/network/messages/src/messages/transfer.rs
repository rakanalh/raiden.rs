use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::{
	deserializers::{
		signature_from_str,
		u256_from_str,
		u64_from_str,
	},
	hashing::hash_balance_data,
	packing::pack_balance_proof,
	serializers::u256_to_str,
	traits::ToBytes,
	types::{
		Address,
		BlockExpiration,
		CanonicalIdentifier,
		ChainID,
		ChannelIdentifier,
		LockedAmount,
		Locksroot,
		MessageIdentifier,
		MessageTypeId,
		PaymentIdentifier,
		Secret,
		SecretHash,
		Signature,
		TokenAddress,
		TokenAmount,
		TokenNetworkAddress,
		H256,
		U256,
	},
};
use raiden_state_machine::types::{
	SendLockExpired,
	SendLockedTransfer,
	SendSecretRequest,
	SendSecretReveal,
	SendUnlock,
};
use serde::{
	Deserialize,
	Serialize,
};
use web3::{
	ethabi::{
		encode,
		Token,
	},
	signing::{
		keccak256,
		SigningError,
	},
};

use super::{
	metadata::Metadata,
	CmdId,
	SignedEnvelopeMessage,
	SignedMessage,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct SecretRequest {
	#[serde(deserialize_with = "u64_from_str")]
	pub message_identifier: MessageIdentifier,
	pub payment_identifier: PaymentIdentifier,
	pub secrethash: SecretHash,
	#[serde(deserialize_with = "u256_from_str")]
	pub amount: TokenAmount,
	pub expiration: BlockExpiration,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
}

impl From<SendSecretRequest> for SecretRequest {
	fn from(event: SendSecretRequest) -> Self {
		Self {
			message_identifier: event.message_identifier,
			payment_identifier: event.payment_identifier,
			secrethash: event.secrethash,
			amount: event.amount,
			expiration: event.expiration,
			signature: Signature::default(),
		}
	}
}

impl SignedMessage for SecretRequest {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let expiration: U256 = self.expiration.into();

		let mut amount = [0u8; 32];
		self.amount.to_big_endian(&mut amount);

		let mut payment_identifier = [0u8; 8];
		self.payment_identifier.to_big_endian(&mut payment_identifier);

		let mut bytes = vec![];
		bytes.extend(&[CmdId::SecretRequest as u8, 0, 0, 0]);
		bytes.extend(&self.message_identifier.to_be_bytes());
		bytes.extend(&payment_identifier);
		bytes.extend(self.secrethash.as_bytes());
		bytes.extend(&amount);
		bytes.extend(&expiration.to_bytes());
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
#[serde(tag = "type", rename = "RevealSecret")]
pub struct SecretReveal {
	#[serde(deserialize_with = "u64_from_str")]
	pub message_identifier: MessageIdentifier,
	pub secret: Secret,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
}

impl From<SendSecretReveal> for SecretReveal {
	fn from(event: SendSecretReveal) -> Self {
		Self {
			message_identifier: event.message_identifier,
			secret: event.secret,
			signature: Signature::default(),
		}
	}
}

impl SignedMessage for SecretReveal {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let message_identifier = self.message_identifier.to_be_bytes();
		let mut bytes = vec![];

		bytes.extend(&[CmdId::RevealSecret as u8, 0, 0, 0]);
		bytes.extend(message_identifier);
		bytes.extend(&self.secret.0);
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
pub struct LockExpired {
	#[serde(deserialize_with = "u64_from_str")]
	pub message_identifier: MessageIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str")]
	pub channel_identifier: U256,
	#[serde(deserialize_with = "u256_from_str")]
	pub transferred_amount: TokenAmount,
	#[serde(deserialize_with = "u256_from_str")]
	pub locked_amount: LockedAmount,
	pub locksroot: Locksroot,
	#[serde(deserialize_with = "u256_from_str")]
	pub nonce: U256,
	pub recipient: Address,
	pub secrethash: SecretHash,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
}

impl From<SendLockExpired> for LockExpired {
	fn from(event: SendLockExpired) -> Self {
		Self {
			message_identifier: event.message_identifier,
			chain_id: event.canonical_identifier.chain_identifier.clone(),
			token_network_address: event.canonical_identifier.token_network_address,
			channel_identifier: event.canonical_identifier.channel_identifier,
			transferred_amount: event.balance_proof.transferred_amount,
			locked_amount: event.balance_proof.locked_amount,
			locksroot: event.balance_proof.locksroot.clone(),
			recipient: event.recipient,
			secrethash: event.secrethash,
			nonce: event.balance_proof.nonce,
			signature: Signature::default(),
		}
	}
}

impl SignedMessage for LockExpired {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let balance_hash =
			hash_balance_data(self.transferred_amount, self.locked_amount, self.locksroot.clone())
				.unwrap();
		pack_balance_proof(
			self.nonce,
			balance_hash,
			self.message_hash(),
			CanonicalIdentifier {
				chain_identifier: self.chain_id,
				token_network_address: self.token_network_address,
				channel_identifier: self.channel_identifier,
			},
			MessageTypeId::BalanceProof,
		)
		.0
	}

	fn bytes_to_pack(&self) -> Vec<u8> {
		vec![]
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

impl SignedEnvelopeMessage for LockExpired {
	fn message_hash(&self) -> H256 {
		let message_identifier = self.message_identifier.to_be_bytes();

		let mut res: Vec<u8> = Vec::new();
		res.push(CmdId::LockExpired as u8);
		res.extend(&message_identifier);
		res.extend(&self.recipient.as_bytes().to_vec());
		res.extend(&self.secrethash.as_bytes().to_vec());

		H256::from_slice(&keccak256(&res))
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct Unlock {
	#[serde(deserialize_with = "u64_from_str")]
	pub message_identifier: MessageIdentifier,
	pub payment_identifier: PaymentIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub channel_identifier: U256,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub transferred_amount: TokenAmount,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub locked_amount: LockedAmount,
	pub locksroot: Locksroot,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub nonce: U256,
	pub secret: Secret,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
}

impl From<SendUnlock> for Unlock {
	fn from(event: SendUnlock) -> Self {
		Self {
			message_identifier: event.message_identifier,
			payment_identifier: event.payment_identifier,
			chain_id: event.canonical_identifier.chain_identifier.clone(),
			token_network_address: event.canonical_identifier.token_network_address,
			channel_identifier: event.canonical_identifier.channel_identifier,
			transferred_amount: event.balance_proof.transferred_amount,
			locked_amount: event.balance_proof.locked_amount,
			locksroot: event.balance_proof.locksroot,
			secret: event.secret,
			nonce: event.balance_proof.nonce,
			signature: Signature::default(),
		}
	}
}

impl SignedMessage for Unlock {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let balance_hash =
			hash_balance_data(self.transferred_amount, self.locked_amount, self.locksroot.clone())
				.unwrap();
		pack_balance_proof(
			self.nonce,
			balance_hash,
			self.message_hash(),
			CanonicalIdentifier {
				chain_identifier: self.chain_id,
				token_network_address: self.token_network_address,
				channel_identifier: self.channel_identifier,
			},
			MessageTypeId::BalanceProof,
		)
		.0
	}

	fn bytes_to_pack(&self) -> Vec<u8> {
		vec![]
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

impl SignedEnvelopeMessage for Unlock {
	fn message_hash(&self) -> H256 {
		let message_identifier = self.message_identifier.to_be_bytes();
		let mut payment_identifier = [0u8; 8];
		self.payment_identifier.to_big_endian(&mut payment_identifier);

		let mut res: Vec<u8> = Vec::new();
		res.push(CmdId::Unlock as u8);
		res.extend(&message_identifier);
		res.extend(&payment_identifier);
		res.extend(&self.secret.0.clone());

		H256::from_slice(&keccak256(&res))
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lock {
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub amount: TokenAmount,
	pub expiration: BlockExpiration,
	pub secrethash: Option<SecretHash>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct LockedTransfer {
	#[serde(deserialize_with = "u64_from_str")]
	pub message_identifier: MessageIdentifier,
	pub payment_identifier: PaymentIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub channel_identifier: ChannelIdentifier,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub transferred_amount: TokenAmount,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub locked_amount: LockedAmount,
	pub locksroot: Locksroot,
	pub token: TokenAddress,
	pub recipient: Address,
	pub lock: Lock,
	pub target: Address,
	pub initiator: Address,
	pub metadata: Metadata,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub nonce: U256,
	pub secret: Option<Secret>,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
}

impl From<SendLockedTransfer> for LockedTransfer {
	fn from(event: SendLockedTransfer) -> Self {
		let metadata: Metadata = event.clone().into();
		Self {
			message_identifier: event.message_identifier,
			payment_identifier: event.transfer.payment_identifier,
			chain_id: event.canonical_identifier.chain_identifier.clone(),
			token_network_address: event.canonical_identifier.token_network_address,
			channel_identifier: event.canonical_identifier.channel_identifier,
			transferred_amount: event.transfer.balance_proof.transferred_amount,
			locked_amount: event.transfer.balance_proof.locked_amount,
			locksroot: event.transfer.balance_proof.locksroot.clone(),
			secret: event.transfer.secret.clone(),
			nonce: event.transfer.balance_proof.nonce,
			signature: Signature::default(),
			token: event.transfer.token,
			recipient: event.recipient,
			lock: Lock {
				amount: event.transfer.lock.amount,
				expiration: event.transfer.lock.expiration,
				secrethash: Some(event.transfer.lock.secrethash),
			},
			target: event.transfer.target,
			initiator: event.transfer.initiator,
			metadata,
		}
	}
}

impl SignedMessage for LockedTransfer {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let balance_hash =
			hash_balance_data(self.transferred_amount, self.locked_amount, self.locksroot.clone())
				.unwrap();
		pack_balance_proof(
			self.nonce,
			balance_hash,
			self.message_hash(),
			CanonicalIdentifier {
				chain_identifier: self.chain_id,
				token_network_address: self.token_network_address,
				channel_identifier: self.channel_identifier,
			},
			MessageTypeId::BalanceProof,
		)
		.0
	}

	fn bytes_to_pack(&self) -> Vec<u8> {
		let mut b = vec![];

		let message_identifier = self.message_identifier.to_be_bytes();
		let mut payment_identifier = [0u8; 8];
		self.payment_identifier.to_big_endian(&mut payment_identifier);
		let lock_expiration: U256 = self.lock.expiration.into();

		b.push(CmdId::LockedTransfer as u8);
		b.extend(message_identifier);
		b.extend(payment_identifier);
		b.extend(lock_expiration.to_bytes());
		b.extend(self.token.as_bytes());
		b.extend(self.recipient.as_bytes());
		b.extend(self.target.as_bytes());
		b.extend(self.initiator.as_bytes());
		if let Some(secrethash) = self.lock.secrethash {
			b.extend(secrethash.as_bytes());
		}
		b.extend(encode(&[Token::Uint(self.lock.amount.into())]));
		b
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

impl SignedEnvelopeMessage for LockedTransfer {
	fn message_hash(&self) -> H256 {
		let mut packed_data = self.bytes_to_pack();
		packed_data.extend_from_slice(&self.metadata.hash().unwrap_or_default());

		H256::from_slice(&keccak256(&packed_data))
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefundTransfer {
	#[serde(deserialize_with = "u64_from_str")]
	pub message_identifier: MessageIdentifier,
	pub payment_identifier: PaymentIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str")]
	pub channel_identifier: U256,
	#[serde(deserialize_with = "u256_from_str")]
	pub transferred_amount: TokenAmount,
	#[serde(deserialize_with = "u256_from_str")]
	pub locked_amount: LockedAmount,
	pub locksroot: Locksroot,
	pub token: TokenAddress,
	pub recipient: Address,
	pub lock: Lock,
	pub target: Address,
	pub initiator: Address,
	pub metadata: Metadata,
	#[serde(deserialize_with = "u256_from_str")]
	pub nonce: U256,
	pub secret: Secret,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
}
