use std::str::FromStr;

use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::{
	deserializers::{
		signature_from_str,
		u256_from_str,
		u32_from_str,
	},
	traits::ToBytes,
	types::{
		message_type::MessageTypeId,
		Address,
		BlockExpiration,
		ChainID,
		Locksroot,
		MessageIdentifier,
		PaymentIdentifier,
		Secret,
		SecretHash,
		TokenAddress,
		TokenAmount,
		TokenNetworkAddress,
		H256,
		U256,
	},
};
use raiden_state_machine::{
	machine::channel::utils::{
		hash_balance_data,
		pack_balance_proof,
	},
	types::{
		CanonicalIdentifier,
		SendLockExpired,
		SendLockedTransfer,
		SendSecretRequest,
		SendSecretReveal,
		SendUnlock,
	},
};
use serde::{
	Deserialize,
	Serialize,
};
use tiny_keccak::{
	Hasher,
	Keccak,
};
use web3::signing::SigningError;

use super::{
	metadata::Metadata,
	CmdId,
	SignedEnvelopeMessage,
	SignedMessage,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretRequest {
	#[serde(deserialize_with = "u32_from_str")]
	pub message_identifier: u32,
	pub payment_identifier: PaymentIdentifier,
	pub secrethash: SecretHash,
	#[serde(deserialize_with = "u256_from_str")]
	pub amount: TokenAmount,
	pub expiration: BlockExpiration,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Vec<u8>,
}

impl From<SendSecretRequest> for SecretRequest {
	fn from(event: SendSecretRequest) -> Self {
		Self {
			message_identifier: event.message_identifier,
			payment_identifier: event.payment_identifier,
			secrethash: event.secrethash,
			amount: event.amount,
			expiration: event.expiration,
			signature: vec![],
		}
	}
}

impl SignedMessage for SecretRequest {
	fn bytes(&self) -> Vec<u8> {
		let cmd_id: [u8; 1] = CmdId::SecretRequest.into();

		let mut expiration = [0u8; 32];
		self.expiration.to_big_endian(&mut expiration);

		let mut amount = [0u8; 32];
		self.amount.to_big_endian(&mut amount);

		let mut bytes = vec![];
		bytes.extend_from_slice(&cmd_id);
		bytes.extend_from_slice(&self.message_identifier.to_be_bytes());
		bytes.append(&mut self.payment_identifier.as_bytes());
		bytes.extend_from_slice(self.secrethash.as_bytes());
		bytes.extend_from_slice(&amount);
		bytes.extend_from_slice(&expiration);
		bytes
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes();
		Ok(())
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretReveal {
	#[serde(deserialize_with = "u32_from_str")]
	pub message_identifier: u32,
	pub secret: Secret,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Vec<u8>,
}

impl From<SendSecretReveal> for SecretReveal {
	fn from(event: SendSecretReveal) -> Self {
		Self {
			message_identifier: event.message_identifier,
			secret: event.secret,
			signature: vec![],
		}
	}
}

impl SignedMessage for SecretReveal {
	fn bytes(&self) -> Vec<u8> {
		let cmd_id: [u8; 1] = CmdId::SecretRequest.into();

		let mut bytes = vec![];
		bytes.extend_from_slice(&cmd_id);
		bytes.extend_from_slice(&self.secret.0);
		bytes
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes();
		Ok(())
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockExpired {
	#[serde(deserialize_with = "u32_from_str")]
	pub message_identifier: u32,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str")]
	pub channel_identifier: U256,
	#[serde(deserialize_with = "u256_from_str")]
	pub transferred_amount: TokenAmount,
	#[serde(deserialize_with = "u256_from_str")]
	pub locked_amount: TokenAmount,
	pub locksroot: Locksroot,
	#[serde(deserialize_with = "u256_from_str")]
	pub nonce: U256,
	pub recipient: Address,
	pub secrethash: SecretHash,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Vec<u8>,
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
			signature: vec![],
		}
	}
}

impl SignedMessage for LockExpired {
	fn bytes(&self) -> Vec<u8> {
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

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes();
		Ok(())
	}
}

impl SignedEnvelopeMessage for LockExpired {
	fn message_hash(&self) -> H256 {
		let cmd: [u8; 1] = CmdId::LockExpired.into();

		let mut res: Vec<u8> = Vec::new();
		res.append(&mut cmd.to_vec());
		res.append(&mut self.message_identifier.to_be_bytes().to_vec());
		res.append(&mut self.recipient.as_bytes().to_vec());
		res.append(&mut self.secrethash.as_bytes().to_vec());

		let mut keccak = Keccak::v256();
		let mut result = [0u8; 32];
		keccak.update(&res);
		keccak.finalize(&mut result);
		H256::from_slice(&result)
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unlock {
	#[serde(deserialize_with = "u32_from_str")]
	pub message_identifier: u32,
	pub payment_identifier: PaymentIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	pub channel_identifier: U256,
	#[serde(deserialize_with = "u256_from_str")]
	pub transferred_amount: TokenAmount,
	#[serde(deserialize_with = "u256_from_str")]
	pub locked_amount: TokenAmount,
	pub locksroot: Locksroot,
	#[serde(deserialize_with = "u256_from_str")]
	pub nonce: U256,
	pub secret: Secret,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Vec<u8>,
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
			signature: vec![],
		}
	}
}

impl SignedMessage for Unlock {
	fn bytes(&self) -> Vec<u8> {
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

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes();
		Ok(())
	}
}

impl SignedEnvelopeMessage for Unlock {
	fn message_hash(&self) -> H256 {
		let cmd: [u8; 1] = CmdId::LockExpired.into();

		let mut res: Vec<u8> = Vec::new();
		res.append(&mut cmd.to_vec());
		res.append(&mut self.message_identifier.to_be_bytes().to_vec());
		res.append(&mut self.payment_identifier.as_bytes().to_vec());
		res.append(&mut self.secret.0.clone());

		let mut keccak = Keccak::v256();
		let mut result = [0u8; 32];
		keccak.update(&res);
		keccak.finalize(&mut result);
		H256::from_slice(&result)
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lock {
	#[serde(deserialize_with = "u256_from_str")]
	pub amount: TokenAmount,
	pub expiration: BlockExpiration,
	pub secrethash: Option<SecretHash>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockedTransfer {
	#[serde(deserialize_with = "u32_from_str")]
	pub message_identifier: MessageIdentifier,
	pub payment_identifier: PaymentIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str")]
	pub channel_identifier: U256,
	#[serde(deserialize_with = "u256_from_str")]
	pub transferred_amount: TokenAmount,
	#[serde(deserialize_with = "u256_from_str")]
	pub locked_amount: TokenAmount,
	pub locksroot: Locksroot,
	pub token: TokenAddress,
	pub recipient: Address,
	pub lock: Lock,
	pub target: Address,
	pub initiator: Address,
	pub metadata: Metadata,
	#[serde(deserialize_with = "u256_from_str")]
	pub nonce: U256,
	pub secret: Option<Secret>,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Vec<u8>,
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
			signature: vec![],
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
	fn bytes(&self) -> Vec<u8> {
		let balance_hash =
			hash_balance_data(self.transferred_amount, self.locked_amount, self.locksroot.clone())
				.unwrap();
		println!("transferred: {:?}", self.transferred_amount);
		println!("locked: {:?}", self.locked_amount);
		let a = TokenAmount::from_str("19000000000000000000").unwrap();
		let value: serde_json::Value = serde_json::to_value(a).unwrap();
		println!("locked 2: {:?}", U256::deserialize(value));
		println!("locksroot: {:?}", hex::encode(self.locksroot.clone().0));
		println!("Balance Hash: {:?}", hex::encode(balance_hash));
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

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes();
		Ok(())
	}
}

impl SignedEnvelopeMessage for LockedTransfer {
	fn message_hash(&self) -> H256 {
		let cmd: [u8; 1] = CmdId::LockExpired.into();

		let mut res: Vec<u8> = Vec::new();
		res.append(&mut cmd.to_vec());
		res.append(&mut self.message_identifier.to_be_bytes().to_vec());
		res.append(&mut self.payment_identifier.as_bytes().to_vec());
		if let Some(secret) = &self.secret {
			res.append(&mut secret.0.clone());
		}

		let mut keccak = Keccak::v256();
		let mut result = [0u8; 32];
		keccak.update(&res);
		keccak.finalize(&mut result);
		H256::from_slice(&result)
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefundTransfer {
	#[serde(deserialize_with = "u32_from_str")]
	pub message_identifier: u32,
	pub payment_identifier: PaymentIdentifier,
	pub chain_id: ChainID,
	pub token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str")]
	pub channel_identifier: U256,
	#[serde(deserialize_with = "u256_from_str")]
	pub transferred_amount: TokenAmount,
	#[serde(deserialize_with = "u256_from_str")]
	pub locked_amount: TokenAmount,
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
	pub signature: Vec<u8>,
}
