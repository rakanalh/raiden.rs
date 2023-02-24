use raiden_blockchain::keys::{
	signature_to_bytes,
	PrivateKey,
};
use raiden_primitives::{
	deserializers::{
		deserialize_signature,
		deserialize_u32_from_str,
	},
	types::{
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

#[derive(Clone, Serialize, Deserialize)]
pub struct SecretRequest {
	message_identifier: u32,
	payment_identifier: PaymentIdentifier,
	secrethash: SecretHash,
	amount: TokenAmount,
	expiration: BlockExpiration,
	signature: Vec<u8>,
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
		bytes.extend_from_slice(self.payment_identifier.as_bytes());
		bytes.extend_from_slice(self.secrethash.as_bytes());
		bytes.extend_from_slice(&amount);
		bytes.extend_from_slice(&expiration);
		bytes
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = signature_to_bytes(self.sign_message(key)?);
		Ok(())
	}
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SecretReveal {
	message_identifier: u32,
	secret: Secret,
	signature: Vec<u8>,
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
		self.signature = signature_to_bytes(self.sign_message(key)?);
		Ok(())
	}
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LockExpired {
	message_identifier: u32,
	chain_id: ChainID,
	token_network_address: TokenNetworkAddress,
	channel_identifier: U256,
	transferred_amount: TokenAmount,
	locked_amount: TokenAmount,
	locksroot: Locksroot,
	nonce: U256,
	recipient: Address,
	secrethash: SecretHash,
	signature: Vec<u8>,
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
		)
		.0
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = signature_to_bytes(self.sign_message(key)?);
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

#[derive(Clone, Serialize, Deserialize)]
pub struct Unlock {
	message_identifier: u32,
	payment_identifier: PaymentIdentifier,
	chain_id: ChainID,
	token_network_address: TokenNetworkAddress,
	channel_identifier: U256,
	transferred_amount: TokenAmount,
	locked_amount: TokenAmount,
	locksroot: Locksroot,
	nonce: U256,
	secret: Secret,
	signature: Vec<u8>,
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
		)
		.0
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = signature_to_bytes(self.sign_message(key)?);
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

#[derive(Clone, Serialize, Deserialize)]
pub struct Lock {
	amount: TokenAmount,
	expiration: BlockExpiration,
	secrethash: SecretHash,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LockedTransfer {
	message_identifier: u32,
	payment_identifier: PaymentIdentifier,
	chain_id: ChainID,
	token_network_address: TokenNetworkAddress,
	channel_identifier: U256,
	transferred_amount: TokenAmount,
	locked_amount: TokenAmount,
	locksroot: Locksroot,
	token: TokenAddress,
	recipient: Address,
	lock: Lock,
	target: Address,
	initiator: Address,
	metadata: Metadata,
	nonce: U256,
	secret: Option<Secret>,
	signature: Vec<u8>,
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
				secrethash: event.transfer.lock.secrethash,
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
		pack_balance_proof(
			self.nonce,
			balance_hash,
			self.message_hash(),
			CanonicalIdentifier {
				chain_identifier: self.chain_id,
				token_network_address: self.token_network_address,
				channel_identifier: self.channel_identifier,
			},
		)
		.0
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = signature_to_bytes(self.sign_message(key)?);
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

#[derive(Clone, Serialize, Deserialize)]
pub struct RefundTransfer {
	message_identifier: u32,
	payment_identifier: PaymentIdentifier,
	chain_id: ChainID,
	token_network_address: TokenNetworkAddress,
	channel_identifier: U256,
	transferred_amount: TokenAmount,
	locked_amount: TokenAmount,
	locksroot: Locksroot,
	token: TokenAddress,
	recipient: Address,
	lock: Lock,
	target: Address,
	initiator: Address,
	metadata: Metadata,
	nonce: U256,
	secret: Secret,
	signature: Vec<u8>,
}
