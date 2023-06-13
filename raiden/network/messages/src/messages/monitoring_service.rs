use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::{
	deserializers::u256_from_str,
	hashing::hash_balance_data,
	packing::{
		pack_balance_proof_message,
		pack_reward_proof,
	},
	serializers::u256_to_str,
	traits::ToBytes,
	types::{
		Address,
		BalanceHash,
		CanonicalIdentifier,
		ChainID,
		ChannelIdentifier,
		MessageHash,
		MessageTypeId,
		Nonce,
		Signature,
		TokenAmount,
		TokenNetworkAddress,
	},
};
use raiden_state_machine::types::BalanceProofState;
use serde::{
	Deserialize,
	Serialize,
};
use web3::signing::SigningError;

use super::SignedMessage;

/// Message sub-field `onchain_balance_proof` for `RequestMonitoring`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignedBlindedBalanceProof {
	chain_id: ChainID,
	token_network_address: TokenNetworkAddress,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	channel_identifier: ChannelIdentifier,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	nonce: Nonce,
	additional_hash: MessageHash,
	balance_hash: BalanceHash,
	signature: Signature,
	non_closing_signature: Signature,
}

impl From<BalanceProofState> for SignedBlindedBalanceProof {
	fn from(balance_proof: BalanceProofState) -> Self {
		Self {
			chain_id: balance_proof.canonical_identifier.chain_identifier,
			token_network_address: balance_proof.canonical_identifier.token_network_address,
			channel_identifier: balance_proof.canonical_identifier.channel_identifier,
			nonce: balance_proof.nonce,
			additional_hash: balance_proof.message_hash.expect("BP message hash should be set"),
			balance_hash: hash_balance_data(
				balance_proof.transferred_amount,
				balance_proof.locked_amount,
				balance_proof.locksroot,
			)
			.expect("Balance hash should be generated"),
			signature: balance_proof.signature.expect("BP Signature should be set"),
			non_closing_signature: Signature::default(),
		}
	}
}

impl SignedMessage for SignedBlindedBalanceProof {
	fn bytes_to_sign(&self) -> Vec<u8> {
		pack_balance_proof_message(
			self.nonce,
			self.balance_hash,
			self.additional_hash,
			CanonicalIdentifier {
				chain_identifier: self.chain_id,
				token_network_address: self.token_network_address,
				channel_identifier: self.channel_identifier,
			},
			MessageTypeId::BalanceProofUpdate,
			self.signature.clone(),
		)
		.0
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.non_closing_signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

/// """Message to request channel watching from a monitoring service.
/// Spec:
///     https://raiden-network-specification.readthedocs.io/en/latest/monitoring_service.html\
///     #monitor-request
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct RequestMonitoring {
	pub balance_proof: SignedBlindedBalanceProof,
	#[serde(deserialize_with = "u256_from_str", serialize_with = "u256_to_str")]
	pub reward_amount: TokenAmount,
	pub monitoring_service_contract_address: Address,
	pub non_closing_participant: Address,
	pub non_closing_signature: Signature,
	pub signature: Signature,
}

impl RequestMonitoring {
	pub fn from_balance_proof(
		balance_proof: BalanceProofState,
		non_closing_participant: Address,
		reward_amount: TokenAmount,
		monitoring_service_contract_address: Address,
	) -> Self {
		let balance_proof: SignedBlindedBalanceProof = balance_proof.into();
		Self {
			reward_amount,
			non_closing_participant,
			monitoring_service_contract_address,
			balance_proof,
			non_closing_signature: Signature::default(),
			signature: Signature::default(),
		}
	}
}

impl SignedMessage for RequestMonitoring {
	fn bytes_to_sign(&self) -> Vec<u8> {
		pack_reward_proof(
			self.monitoring_service_contract_address,
			self.balance_proof.chain_id,
			self.balance_proof.token_network_address,
			self.non_closing_participant,
			self.non_closing_signature.clone(),
			self.reward_amount,
		)
		.0
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.balance_proof.sign(key.clone())?;
		self.non_closing_signature = self.balance_proof.non_closing_signature.clone();
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}
