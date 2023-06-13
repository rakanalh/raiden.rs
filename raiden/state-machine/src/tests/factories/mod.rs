mod builder;
mod generator;
mod keyring;

pub use builder::*;
use ethsign::SecretKey;
pub use generator::*;
pub use keyring::*;
use raiden_primitives::{
	hashing::hash_balance_data,
	packing::pack_balance_proof,
	signing::hash_data,
	traits::ToBytes,
	types::{
		Address,
		Bytes,
		CanonicalIdentifier,
		Locksroot,
		MessageTypeId,
		Nonce,
		TokenAmount,
		H256,
	},
};
use web3::signing::{
	Signature,
	SigningError,
};

use crate::types::BalanceProofState;

pub fn sign_message(secret: SecretKey, message: &[u8]) -> Result<Signature, SigningError> {
	let data_hash = hash_data(message);
	let signature = secret.sign(&data_hash).expect("Data should be signed");

	Ok(Signature {
		r: H256::from(signature.r),
		s: H256::from(signature.s),
		v: signature.v as u64 + 27,
	})
}

pub fn make_balance_proof(
	secret_key: SecretKey,
	canonical_identifier: CanonicalIdentifier,
	locked_amount: TokenAmount,
	locksroot: Locksroot,
	transferred_amount: TokenAmount,
	sender: Address,
	nonce: Nonce,
) -> BalanceProofState {
	let balance_hash = hash_balance_data(transferred_amount, locked_amount, locksroot)
		.expect("Should generate balance hash");
	let packed_data = pack_balance_proof(
		nonce,
		balance_hash,
		H256::zero(),
		canonical_identifier.clone(),
		MessageTypeId::BalanceProof,
	);
	let signature = sign_message(secret_key, &packed_data.0)
		.expect("Should generate signature")
		.to_bytes();

	BalanceProofState {
		nonce,
		transferred_amount,
		locked_amount,
		locksroot,
		canonical_identifier,
		balance_hash,
		message_hash: Some(H256::zero()),
		signature: Some(Bytes(signature)),
		sender: Some(sender),
	}
}
