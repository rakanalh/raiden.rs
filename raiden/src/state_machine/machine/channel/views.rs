use std::{cmp::max, ops::Mul};

use web3::types::Bytes;

use crate::{
	constants::{DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS, LOCKSROOT_OF_NO_LOCKS},
	primitives::{
		BalanceProofData, BlockExpiration, BlockNumber, LockTimeout, LockedAmount, Nonce,
		RevealTimeout, SecretHash, TokenAmount,
	},
	state_machine::types::{ChannelEndState, HashTimeLockState},
};

pub(super) fn get_next_nonce(end_state: &ChannelEndState) -> Nonce {
	end_state.nonce + 1
}

pub fn balance(
	sender: &ChannelEndState,
	receiver: &ChannelEndState,
	subtract_withdraw: bool,
) -> TokenAmount {
	let mut sender_transferred_amount = TokenAmount::zero();
	let mut receiver_transferred_amount = TokenAmount::zero();

	if let Some(sender_balance_proof) = &sender.balance_proof {
		sender_transferred_amount = sender_balance_proof.transferred_amount;
	}

	if let Some(receiver_balance_proof) = &receiver.balance_proof {
		receiver_transferred_amount = receiver_balance_proof.transferred_amount;
	}

	let max_withdraw = max(sender.offchain_total_withdraw(), sender.onchain_total_withdraw);
	let withdraw = if subtract_withdraw { max_withdraw } else { TokenAmount::zero() };

	sender.contract_balance - withdraw - sender_transferred_amount + receiver_transferred_amount
}

/// Calculates the maximum "total_withdraw_amount" for a channel.
/// This will leave the channel without funds, when this is withdrawn from the contract,
/// or pending as offchain-withdraw.
pub(super) fn get_max_withdraw_amount(
	sender_state: &ChannelEndState,
	receiver_state: &ChannelEndState,
) -> TokenAmount {
	balance(sender_state, receiver_state, false)
}

pub(crate) fn get_safe_initial_expiration(
	block_number: BlockNumber,
	reveal_timeout: RevealTimeout,
	lock_timeout: Option<LockTimeout>,
) -> BlockNumber {
	if let Some(lock_timeout) = lock_timeout {
		return block_number + lock_timeout
	}

	block_number + (reveal_timeout * 2)
}

pub(super) fn get_amount_locked(end_state: &ChannelEndState) -> LockedAmount {
	let total_pending: TokenAmount = end_state
		.secrethashes_to_lockedlocks
		.values()
		.map(|lock| lock.amount)
		.fold(LockedAmount::zero(), |acc, x| acc.saturating_add(x));
	let total_unclaimed: TokenAmount = end_state
		.secrethashes_to_unlockedlocks
		.values()
		.map(|lock| lock.amount)
		.fold(LockedAmount::zero(), |acc, x| acc.saturating_add(x));
	let total_unclaimed_onchain = end_state
		.secrethashes_to_onchain_unlockedlocks
		.values()
		.map(|lock| lock.amount)
		.fold(LockedAmount::zero(), |acc, x| acc.saturating_add(x));

	total_pending + total_unclaimed + total_unclaimed_onchain
}

pub(super) fn get_current_balance_proof(end_state: &ChannelEndState) -> BalanceProofData {
	if let Some(balance_proof) = &end_state.balance_proof {
		(
			balance_proof.locksroot.clone(),
			end_state.nonce,
			balance_proof.transferred_amount,
			get_amount_locked(end_state),
		)
	} else {
		(
			Bytes(LOCKSROOT_OF_NO_LOCKS.to_vec()),
			Nonce::zero(),
			TokenAmount::zero(),
			LockedAmount::zero(),
		)
	}
}

pub(crate) fn get_sender_expiration_threshold(expiration: BlockExpiration) -> BlockExpiration {
	expiration + DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS.mul(2).into()
}

pub(crate) fn get_receiver_expiration_threshold(expiration: BlockExpiration) -> BlockExpiration {
	expiration + DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS.into()
}

pub(crate) fn get_lock(
	end_state: &ChannelEndState,
	secrethash: SecretHash,
) -> Option<HashTimeLockState> {
	let mut lock = end_state.secrethashes_to_lockedlocks.get(&secrethash);
	if lock.is_none() {
		lock = end_state.secrethashes_to_unlockedlocks.get(&secrethash).map(|lock| &lock.lock);
	}
	if lock.is_none() {
		lock = end_state
			.secrethashes_to_onchain_unlockedlocks
			.get(&secrethash)
			.map(|lock| &lock.lock);
	}
	lock.cloned()
}
