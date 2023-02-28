use ethabi::{
	encode,
	ethereum_types::U256,
	Token,
};
use raiden_primitives::types::{
	message_type::MessageTypeId,
	Address,
	BalanceHash,
	BlockExpiration,
	Bytes,
	LockedAmount,
	Locksroot,
	MessageHash,
	Nonce,
	TokenAmount,
	H256,
};
use web3::signing::keccak256;

use crate::types::{
	CanonicalIdentifier,
	HashTimeLockState,
	PendingLocksState,
};

pub(super) fn compute_locks_with(
	pending_locks: &mut PendingLocksState,
	lock: HashTimeLockState,
) -> Option<PendingLocksState> {
	if !pending_locks.locks.contains(&lock.encoded) {
		let mut locks = PendingLocksState { locks: pending_locks.locks.clone() };
		locks.locks.push(lock.encoded);
		return Some(locks)
	}

	None
}

pub(super) fn compute_locks_without(
	pending_locks: &mut PendingLocksState,
	lock: &HashTimeLockState,
) -> Option<PendingLocksState> {
	if pending_locks.locks.contains(&lock.encoded) {
		let mut locks = PendingLocksState { locks: pending_locks.locks.clone() };
		locks.locks.retain(|l| l != &lock.encoded);
		return Some(locks)
	}

	None
}

pub(super) fn compute_locksroot(locks: &PendingLocksState) -> Locksroot {
	let locks: Vec<&[u8]> = locks.locks.iter().map(|lock| lock.0.as_slice()).collect();
	let hash = keccak256(&locks.concat());
	return Bytes(hash.to_vec())
}

pub fn hash_balance_data(
	transferred_amount: TokenAmount,
	locked_amount: LockedAmount,
	locksroot: Locksroot,
) -> Result<BalanceHash, String> {
	if locksroot == Bytes(vec![]) {
		return Err("Can't hash empty locksroot".to_string())
	}

	if locksroot.0.len() != 32 {
		return Err("Locksroot has wrong length".to_string())
	}

	let mut transferred_amount_in_bytes: [u8; 32] = [0; 32];
	transferred_amount.to_big_endian(&mut transferred_amount_in_bytes);

	let mut locked_amount_in_bytes: [u8; 32] = [0; 32];
	locked_amount.to_big_endian(&mut locked_amount_in_bytes);

	let hash = keccak256(
		&[&transferred_amount_in_bytes[..], &locked_amount_in_bytes[..], &locksroot.0[..]].concat(),
	);
	Ok(H256::from_slice(&hash))
}

pub fn pack_balance_proof(
	nonce: Nonce,
	balance_hash: BalanceHash,
	additional_hash: MessageHash,
	canonical_identifier: CanonicalIdentifier,
	msg_type: MessageTypeId,
) -> Bytes {
	let mut b = vec![];

	b.extend(canonical_identifier.token_network_address.as_bytes());
	b.extend(encode(&[Token::Uint(canonical_identifier.chain_identifier.into())]));
	b.extend(encode(&[Token::Uint(U256::from(msg_type as u8))]));
	b.extend(encode(&[Token::Uint(canonical_identifier.channel_identifier)]));
	b.extend(balance_hash.as_bytes());
	b.extend(encode(&[Token::Uint(nonce)]));
	b.extend(additional_hash.as_bytes());

	Bytes(b)
}

pub fn pack_withdraw(
	canonical_identifier: CanonicalIdentifier,
	participant: Address,
	total_withdraw: TokenAmount,
	expiration_block: BlockExpiration,
) -> Bytes {
	let mut b = vec![];

	b.extend(encode(&[
		Token::Address(canonical_identifier.token_network_address),
		Token::Uint(canonical_identifier.chain_identifier.into()),
		Token::Uint(canonical_identifier.channel_identifier),
		Token::Address(participant),
		Token::Uint(total_withdraw),
		Token::Uint(expiration_block.into()),
	]));

	Bytes(b)
}
