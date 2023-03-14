use sha2::{
	Digest,
	Sha256,
};
use web3::signing::keccak256;

use crate::{
	constants::LOCKSROOT_OF_NO_LOCKS,
	types::{
		BalanceHash,
		LockedAmount,
		Locksroot,
		TokenAmount,
	},
};

pub fn hash_secret(secret: &[u8]) -> [u8; 32] {
	let mut hasher = Sha256::new();
	hasher.update(secret);
	hasher.finalize().into()
}

pub fn hash_balance_data(
	transferred_amount: TokenAmount,
	locked_amount: LockedAmount,
	locksroot: Locksroot,
) -> Result<BalanceHash, String> {
	if locksroot.is_zero() {
		return Err("Can't hash empty locksroot".to_string())
	}

	if locksroot.0.len() != 32 {
		return Err("Locksroot has wrong length".to_string())
	}

	if transferred_amount == TokenAmount::zero() &&
		locked_amount == TokenAmount::zero() &&
		locksroot == *LOCKSROOT_OF_NO_LOCKS
	{
		return Ok(BalanceHash::zero())
	}

	let mut transferred_amount_in_bytes: [u8; 32] = [0; 32];
	transferred_amount.to_big_endian(&mut transferred_amount_in_bytes);

	let mut locked_amount_in_bytes: [u8; 32] = [0; 32];
	locked_amount.to_big_endian(&mut locked_amount_in_bytes);

	let hash = keccak256(
		&[&transferred_amount_in_bytes[..], &locked_amount_in_bytes[..], &locksroot.0[..]].concat(),
	);
	Ok(BalanceHash::from_slice(&hash))
}
