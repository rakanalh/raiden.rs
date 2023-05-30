#![warn(clippy::missing_docs_in_private_items)]

use raiden_primitives::types::Locksroot;
use web3::signing::keccak256;

use crate::types::{
	HashTimeLockState,
	PendingLocksState,
};

pub(crate) fn compute_locks_with(
	pending_locks: &PendingLocksState,
	lock: HashTimeLockState,
) -> Option<PendingLocksState> {
	if !pending_locks.locks.contains(&lock.encoded) {
		let mut locks = PendingLocksState { locks: pending_locks.locks.clone() };
		locks.locks.push(lock.encoded);
		return Some(locks)
	}

	None
}

pub(crate) fn compute_locks_without(
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

pub fn compute_locksroot(locks: &PendingLocksState) -> Locksroot {
	let locks: Vec<&[u8]> = locks.locks.iter().map(|lock| lock.0.as_slice()).collect();
	let hash = keccak256(&locks.concat());
	Locksroot::from_slice(&hash)
}
