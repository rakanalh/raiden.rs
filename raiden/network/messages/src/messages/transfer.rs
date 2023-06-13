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

/// Requests the secret/preimage which unlocks a lock.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct SecretRequest {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
	pub message_identifier: MessageIdentifier,
	pub payment_identifier: PaymentIdentifier,
	pub secrethash: SecretHash,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
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

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

/// Reveal the lock's secret.
///
/// This message is not sufficient to unlock a lock, refer to the Unlock.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "RevealSecret")]
pub struct SecretReveal {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
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

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

/// Message used when a lock expires.
///
/// This will complete an unsuccessful transfer off-chain.
///
/// For this message to be valid the balance proof has to be updated to:
///
/// - Remove the expired lock from the pending locks and reflect it in the locksroot.
/// - Decrease the locked_amount by exactly by lock.amount. If less tokens are decreased the sender
///   may get tokens locked. If more tokens are decreased the recipient will reject the message as
///   on-chain unlocks may fail.
/// This message is necessary for synchronization since other messages may be
/// in-flight, vide Unlock for examples.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct LockExpired {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
	pub message_identifier: MessageIdentifier,
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
	#[serde(skip_serializing)]
	pub recipient: Address,
	pub secrethash: SecretHash,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
}

impl From<SendLockExpired> for LockExpired {
	fn from(event: SendLockExpired) -> Self {
		Self {
			message_identifier: event.message_identifier,
			chain_id: event.canonical_identifier.chain_identifier,
			token_network_address: event.canonical_identifier.token_network_address,
			channel_identifier: event.canonical_identifier.channel_identifier,
			transferred_amount: event.balance_proof.transferred_amount,
			locked_amount: event.balance_proof.locked_amount,
			locksroot: event.balance_proof.locksroot,
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
			hash_balance_data(self.transferred_amount, self.locked_amount, self.locksroot).unwrap();
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

/// Message used to successfully unlock a lock.
///
/// For this message to be valid the balance proof has to be updated to:
///
/// - Remove the successful lock from the pending locks and decrement the locked_amount by the
///   lock's amount, otherwise the sender will pay twice.
/// - Increase the transferred_amount, otherwise the recipient will reject it because it is not
///   being paid.
/// This message is needed to unlock off-chain transfers for channels that used
/// less frequently then the pending locks' expiration, otherwise the receiving
/// end would have to go on-chain to register the secret.
///
/// This message is needed in addition to the RevealSecret to fix
/// synchronization problems. The recipient can not preemptively update its
/// channel state because there may other messages in-flight. Consider the
/// following case:
///
/// 1. Node A sends a LockedTransfer to B.
/// 2. Node B forwards and eventually receives the secret
/// 3. Node A sends a second LockedTransfer to B.
///
/// At point 3, node A had no knowledge about the first payment having its
/// secret revealed, therefore the pending locks from message at step 3 will
/// include both locks. If B were to preemptively remove the lock it would
/// reject the message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct Unlock {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
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
			chain_id: event.canonical_identifier.chain_identifier,
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
			hash_balance_data(self.transferred_amount, self.locked_amount, self.locksroot).unwrap();
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

/// The lock datastructure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Lock {
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub amount: TokenAmount,
	pub expiration: BlockExpiration,
	pub secrethash: Option<SecretHash>,
}

/// Message used to reserve tokens for a new mediated transfer.
///
/// For this message to be valid, the sender must:
///
/// - Use a lock.amount smaller then its current capacity. If the amount is higher, then the
///   recipient will reject it, as it means spending money it does not own.
/// - Have the new lock represented in locksroot.
/// - Increase the locked_amount by exactly `lock.amount` otherwise the message would be rejected by
///   the recipient. If the locked_amount is increased by more, then funds may get locked in the
///   channel. If the locked_amount is increased by less, then the recipient will reject the message
///   as it may mean it received the funds with an on-chain unlock.
/// The initiator will estimate the fees based on the available routes and
/// incorporate it in the lock's amount. Note that with permissive routing it
/// is not possible to predetermine the exact fee amount, as the initiator does
/// not know which nodes are available, thus an estimated value is used.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct LockedTransfer {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
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
	#[serde(skip_serializing)]
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
			chain_id: event.canonical_identifier.chain_identifier,
			token_network_address: event.canonical_identifier.token_network_address,
			channel_identifier: event.canonical_identifier.channel_identifier,
			transferred_amount: event.transfer.balance_proof.transferred_amount,
			locked_amount: event.transfer.balance_proof.locked_amount,
			locksroot: event.transfer.balance_proof.locksroot,
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
			hash_balance_data(self.transferred_amount, self.locked_amount, self.locksroot)
				.expect("Balance hash should be generated");
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
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

impl SignedEnvelopeMessage for LockedTransfer {
	fn message_hash(&self) -> H256 {
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
		b.extend(encode(&[Token::Uint(self.lock.amount)]));
		b.extend_from_slice(&self.metadata.hash().unwrap_or_default());

		H256::from_slice(&keccak256(&b))
	}
}

/// A message used when a payee does not have any available routes to
/// forward the transfer.
///
/// This message is used by the payee to refund the payer when no route is
/// available. This transfer refunds the payer, allowing him to try a new path
/// to complete the transfer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RefundTransfer {
	#[serde(deserialize_with = "u64_from_str")]
	#[serde(skip_serializing)]
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
	pub token: TokenAddress,
	#[serde(skip_serializing)]
	pub recipient: Address,
	pub lock: Lock,
	pub target: Address,
	pub initiator: Address,
	pub metadata: Metadata,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub nonce: U256,
	pub secret: Secret,
	#[serde(deserialize_with = "signature_from_str")]
	pub signature: Signature,
}
