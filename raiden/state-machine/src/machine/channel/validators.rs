use raiden_primitives::types::{
	Address,
	BlockExpiration,
	BlockNumber,
	Bytes,
	MessageHash,
	SecretHash,
	Signature,
	TokenAmount,
};
use web3::{
	signing::recover,
	types::Recovery,
};

use super::{
	utils::{
		compute_locks_with,
		compute_locks_without,
		compute_locksroot,
		hash_balance_data,
		pack_balance_proof,
		pack_withdraw,
	},
	views::{
		get_current_balance_proof,
		get_lock,
		get_next_nonce,
		get_receiver_expiration_threshold,
		get_sender_expiration_threshold,
	},
};
use crate::{
	constants::MAXIMUM_PENDING_TRANSFERS,
	types::{
		ActionChannelWithdraw,
		BalanceProofState,
		CanonicalIdentifier,
		ChannelEndState,
		ChannelState,
		ChannelStatus,
		HashTimeLockState,
		LockedTransferState,
		PendingLocksState,
		PendingWithdrawState,
		ReceiveLockExpired,
		ReceiveTransferRefund,
		ReceiveUnlock,
		ReceiveWithdrawConfirmation,
		ReceiveWithdrawExpired,
		ReceiveWithdrawRequest,
	},
	views,
};

pub(crate) fn is_lock_expired(
	end_state: &ChannelEndState,
	lock: &HashTimeLockState,
	block_number: BlockNumber,
	lock_expiration_threshold: BlockExpiration,
) -> Result<(), String> {
	let secret_registered_on_chain =
		end_state.secrethashes_to_onchain_unlockedlocks.get(&lock.secrethash).is_some();

	if secret_registered_on_chain {
		return Err("Lock has been unlocked onchain".to_owned())
	}

	if block_number < lock_expiration_threshold {
		return Err(format!(
			"Current block number ({}) is not larger than \
             lock.expiration + confirmation blocks ({})",
			block_number, lock_expiration_threshold
		))
	}

	Ok(())
}

pub(crate) fn is_lock_pending(end_state: &ChannelEndState, secrethash: SecretHash) -> bool {
	end_state.secrethashes_to_lockedlocks.contains_key(&secrethash) ||
		end_state.secrethashes_to_unlockedlocks.contains_key(&secrethash) ||
		end_state.secrethashes_to_onchain_unlockedlocks.contains_key(&secrethash)
}

pub(super) fn is_lock_locked(end_state: &ChannelEndState, secrethash: SecretHash) -> bool {
	end_state.secrethashes_to_lockedlocks.contains_key(&secrethash)
}

pub(super) fn is_valid_signature(
	data: Bytes,
	signature: Signature,
	sender_address: Address,
) -> Result<(), String> {
	let recovery =
		Recovery::from_raw_signature(data.0.as_slice(), signature).map_err(|e| e.to_string())?;
	let recovery_id = match recovery.recovery_id() {
		Some(id) => id,
		None => return Err("Found invalid recovery ID".to_owned()),
	};
	let signer_address = recover(data.0.as_slice(), signature.as_bytes(), recovery_id)
		.map_err(|e| format!("Error recovering signature {:?}", e))?;

	if signer_address == sender_address {
		return Ok(())
	}

	return Err("Signature was valid but the expected address does not match".to_owned())
}

pub(super) fn is_valid_balance_proof_signature(
	balance_proof: &BalanceProofState,
	sender_address: Address,
) -> Result<(), String> {
	let balance_hash = hash_balance_data(
		balance_proof.transferred_amount,
		balance_proof.locked_amount,
		balance_proof.locksroot.clone(),
	)?;
	let message_hash = match balance_proof.message_hash {
		Some(hash) => hash,
		None => MessageHash::zero(),
	};
	let data_that_was_signed = pack_balance_proof(
		balance_proof.nonce,
		balance_hash,
		message_hash,
		balance_proof.canonical_identifier.clone(),
	);

	let signature = match balance_proof.signature {
		Some(signature) => signature,
		None => return Err("Balance proof must be signed".to_owned()),
	};

	is_valid_signature(data_that_was_signed, signature, sender_address)
}

pub(super) fn is_balance_proof_safe_for_onchain_operations(
	balance_proof: &BalanceProofState,
) -> bool {
	balance_proof
		.transferred_amount
		.checked_add(balance_proof.locked_amount)
		.is_some()
}

pub(crate) fn is_transfer_expired(
	transfer: &LockedTransferState,
	affected_channel: &ChannelState,
	block_number: BlockNumber,
) -> bool {
	let lock_expiration_threshold = get_sender_expiration_threshold(transfer.lock.expiration);

	is_lock_expired(
		&affected_channel.our_state,
		&transfer.lock,
		block_number,
		lock_expiration_threshold,
	)
	.is_ok()
}

pub(super) fn is_balance_proof_usable_onchain(
	received_balance_proof: &BalanceProofState,
	channel_state: &ChannelState,
	sender_state: &ChannelEndState,
) -> Result<(), String> {
	let expected_nonce = get_next_nonce(sender_state);

	let is_valid_signature =
		is_valid_balance_proof_signature(&received_balance_proof, sender_state.address);

	if channel_state.status() != ChannelStatus::Opened {
		return Err("The channel is already closed.".to_owned())
	} else if received_balance_proof.canonical_identifier != channel_state.canonical_identifier {
		return Err("Canonical identifier does not match".to_owned())
	} else if !is_balance_proof_safe_for_onchain_operations(&received_balance_proof) {
		return Err("Balance proof total transferred amount would overflow onchain.".to_owned())
	} else if received_balance_proof.nonce != expected_nonce {
		return Err(format!(
			"Nonce did not change sequentially. \
                            Expected: {} \
                            got: {}",
			expected_nonce, received_balance_proof.nonce
		))
	}
	is_valid_signature
}

pub(super) fn is_valid_lock_expired(
	channel_state: &ChannelState,
	state_change: ReceiveLockExpired,
	sender_state: &ChannelEndState,
	receiver_state: &ChannelEndState,
	block_number: BlockNumber,
) -> Result<PendingLocksState, String> {
	let secrethash = state_change.secrethash;
	let received_balance_proof = state_change.balance_proof;
	let lock = channel_state
		.partner_state
		.secrethashes_to_lockedlocks
		.get(&secrethash)
		.or_else(|| {
			channel_state
				.partner_state
				.secrethashes_to_unlockedlocks
				.get(&secrethash)
				.map(|lock| &lock.lock)
		});

	let secret_registered_on_chain = channel_state
		.partner_state
		.secrethashes_to_onchain_unlockedlocks
		.contains_key(&secrethash);
	let (_, _, current_transferred_amount, current_locked_amount) =
		get_current_balance_proof(sender_state);
	let is_valid_balance_proof =
		is_balance_proof_usable_onchain(&received_balance_proof, channel_state, sender_state);

	let (lock, expected_locked_amount) = match lock {
		Some(lock) => {
			let expected_locked_amount = current_locked_amount - lock.amount;
			(lock, expected_locked_amount)
		},
		None =>
			return Err(format!(
				"Invalid LockExpired message. \
                                Lock with secrethash {} is not known",
				secrethash
			)),
	};
	let pending_locks = match compute_locks_without(&mut sender_state.pending_locks.clone(), lock) {
		Some(pending_locks) => pending_locks,
		None => return Err(format!("Invalid LockExpired message. Same lock handled twice.")),
	};

	if secret_registered_on_chain {
		return Err(format!("Invalid LockExpired message. Lock was unlocked on-chain"))
	} else if let Err(e) = is_valid_balance_proof {
		return Err(format!("Invalid LockExpired message. {}", e))
	}

	let locksroot_without_lock = compute_locksroot(&pending_locks);
	let check_lock_expired = is_lock_expired(
		receiver_state,
		lock,
		block_number,
		get_receiver_expiration_threshold(lock.expiration),
	);

	if let Err(e) = check_lock_expired {
		return Err(format!("Invalid LockExpired message. {}", e))
	} else if received_balance_proof.locksroot != locksroot_without_lock {
		return Err(format!(
			"Invalid LockExpired message. \
                            Balance proof's locksroot didn't match. \
                            expected {:?} \
                            got {:?}",
			locksroot_without_lock, received_balance_proof.locksroot
		))
	} else if received_balance_proof.transferred_amount != current_transferred_amount {
		return Err(format!(
			"Invalid LockExpired message. \
                            Balance proof's transferred amount changed. \
                            expected {} \
                            got {}",
			current_transferred_amount, received_balance_proof.transferred_amount
		))
	} else if received_balance_proof.locked_amount != expected_locked_amount {
		return Err(format!(
			"Invalid LockExpired message. \
                            Balance proof's locked amount changed. \
                            expected {} \
                            got {}",
			expected_locked_amount, received_balance_proof.locked_amount
		))
	}

	Ok(pending_locks)
}

pub(super) fn valid_locked_transfer_check(
	channel_state: &ChannelState,
	sender_state: &mut ChannelEndState,
	receiver_state: &ChannelEndState,
	message: &'static str,
	received_balance_proof: &BalanceProofState,
	lock: &HashTimeLockState,
) -> Result<PendingLocksState, String> {
	let (_, _, current_transferred_amount, current_locked_amount) =
		get_current_balance_proof(sender_state);
	let distributable = views::channel_distributable(sender_state, receiver_state);
	let expected_locked_amount = current_locked_amount + lock.amount;

	if let Err(e) =
		is_balance_proof_usable_onchain(&received_balance_proof, channel_state, sender_state)
	{
		return Err(format!("Invalid {} message. {}", message, e))
	}

	let pending_locks = match compute_locks_with(&mut sender_state.pending_locks, lock.clone()) {
		Some(pending_locks) => {
			if pending_locks.locks.len() > MAXIMUM_PENDING_TRANSFERS {
				return Err(format!(
					"Invalid {} message. \
                                    Adding the transfer would exceed the allowed limit of {} \
                                    pending transfers per channel.",
					message, MAXIMUM_PENDING_TRANSFERS
				))
			}
			pending_locks
		},
		None => return Err(format!("Invalid {} message. Same lock handled twice", message)),
	};

	let locksroot_with_lock = compute_locksroot(&pending_locks);
	if received_balance_proof.locksroot != locksroot_with_lock {
		return Err(format!(
			"Invalid {} message. Balance proof's lock didn't match. \
                            expected: {:?} \
                            got: {:?}",
			message, locksroot_with_lock, received_balance_proof.locksroot
		))
	} else if received_balance_proof.transferred_amount != current_transferred_amount {
		return Err(format!(
			"Invalid {} message. Balance proof's transferred_amount changed. \
                            expected: {} \
                            got: {}",
			message, current_transferred_amount, received_balance_proof.transferred_amount
		))
	} else if received_balance_proof.locked_amount != expected_locked_amount {
		return Err(format!(
			"Invalid {} message. Balance proof's locked_amount changed. \
                            expected: {} \
                            got: {}",
			message, expected_locked_amount, received_balance_proof.locked_amount
		))
	} else if lock.amount > distributable {
		return Err(format!(
			"Invalid {} message. Lock amount larger than the available distributable. \
                            Lock amount: {}, maximum distributable: {}",
			message, lock.amount, distributable
		))
	}

	Ok(pending_locks)
}

pub(super) fn is_valid_locked_transfer(
	transfer_state: &LockedTransferState,
	channel_state: &ChannelState,
	sender_end_state: &mut ChannelEndState,
	receiver_end_state: &ChannelEndState,
) -> Result<PendingLocksState, String> {
	valid_locked_transfer_check(
		channel_state,
		sender_end_state,
		receiver_end_state,
		"LockedTransfer",
		&transfer_state.balance_proof,
		&transfer_state.lock,
	)
}

pub(super) fn is_valid_total_withdraw(
	channel_state: &ChannelState,
	our_total_withdraw: TokenAmount,
	allow_zero: bool,
) -> Result<(), String> {
	let balance = views::channel_balance(&channel_state.our_state, &channel_state.partner_state);

	if our_total_withdraw.checked_add(channel_state.partner_total_withdraw()).is_none() {
		return Err(format!("The new total_withdraw {} will cause an overflow", our_total_withdraw))
	}

	let withdraw_amount = our_total_withdraw - channel_state.our_total_withdraw();

	if channel_state.status() != ChannelStatus::Opened {
		return Err(format!("Invalid withdraw, the channel is not opened"))
	} else if withdraw_amount < TokenAmount::zero() {
		return Err(format!("Total withdraw {} decreased", our_total_withdraw))
	} else if !allow_zero && withdraw_amount == TokenAmount::zero() {
		return Err(format!("Total withdraw {} did not increase", our_total_withdraw))
	} else if balance < withdraw_amount {
		return Err(format!(
			"Insufficient balance: {}. Request {} for withdraw",
			balance, our_total_withdraw
		))
	}

	return Ok(())
}

pub(super) fn is_valid_withdraw_signature(
	canonical_identifier: CanonicalIdentifier,
	sender: Address,
	participant: Address,
	total_withdraw: TokenAmount,
	expiration_block: BlockExpiration,
	withdraw_signature: Signature,
) -> Result<(), String> {
	let packed = pack_withdraw(canonical_identifier, participant, total_withdraw, expiration_block);
	is_valid_signature(packed, withdraw_signature, sender)
}

/// Determine whether a withdraw has expired.
///
/// The withdraw has expired if the current block exceeds
/// the withdraw's expiration + confirmation blocks.
pub(super) fn is_withdraw_expired(
	block_number: BlockNumber,
	expiration_threshold: BlockExpiration,
) -> bool {
	block_number >= expiration_threshold
}

pub(super) fn is_valid_withdraw_expired(
	channel_state: &ChannelState,
	state_change: &ReceiveWithdrawExpired,
	withdraw_state: &PendingWithdrawState,
	block_number: BlockNumber,
) -> Result<(), String> {
	let expected_nonce = get_next_nonce(&channel_state.partner_state);
	let is_withdraw_expired = is_withdraw_expired(
		block_number,
		get_receiver_expiration_threshold(withdraw_state.expiration),
	);

	if !is_withdraw_expired {
		return Err(format!(
			"WithdrawExpired for withdraw that has not yet expired {}",
			state_change.total_withdraw
		))
	} else if channel_state.canonical_identifier != state_change.canonical_identifier {
		return Err(format!("Invalid canonical identifier provided in withdraw request"))
	} else if state_change.sender != channel_state.partner_state.address {
		return Err(format!("Invalid sender. Request must be sent by the partner"))
	} else if state_change.total_withdraw != withdraw_state.total_withdraw {
		return Err(format!(
			"WithdrawExpired for local withdraw amounts do not match. \
                            Received {}, local amount {}",
			state_change.total_withdraw, withdraw_state.total_withdraw
		))
	} else if state_change.nonce != expected_nonce {
		return Err(format!(
			"Nonce did not change sequentially. Expected: {}, got {}",
			expected_nonce, state_change.nonce
		))
	}

	Ok(())
}

pub(super) fn is_valid_withdraw_request(
	channel_state: &ChannelState,
	withdraw_request: &ReceiveWithdrawRequest,
) -> Result<(), String> {
	let expected_nonce = get_next_nonce(&channel_state.partner_state);
	let balance = views::channel_balance(&channel_state.partner_state, &channel_state.our_state);

	let is_valid = is_valid_withdraw_signature(
		withdraw_request.canonical_identifier.clone(),
		withdraw_request.sender,
		withdraw_request.participant,
		withdraw_request.total_withdraw,
		withdraw_request.expiration,
		withdraw_request.signature,
	);

	let withdraw_amount = withdraw_request.total_withdraw - channel_state.partner_total_withdraw();

	if withdraw_request
		.total_withdraw
		.checked_add(channel_state.our_total_withdraw())
		.is_none()
	{
		return Err(format!(
			"The new total_withdraw {} will cause an overflow",
			withdraw_request.total_withdraw
		))
	}

	// The confirming node must accept an expired withdraw request. This is
	// necessary to clear the requesting node's queue. This is not a security
	// flaw because the smart contract will not allow the withdraw to happen.
	if channel_state.canonical_identifier != withdraw_request.canonical_identifier {
		return Err(format!("Invalid canonical identifier provided in withdraw request"))
	} else if withdraw_request.participant != channel_state.partner_state.address {
		return Err(format!("Invalid participant. It must be the partner's address"))
	} else if withdraw_request.sender != channel_state.partner_state.address {
		return Err(format!("Invalid sender. Request must be sent by the partner"))
	} else if withdraw_amount < TokenAmount::zero() {
		return Err(format!("Total withdraw {} decreased", withdraw_request.total_withdraw))
	} else if balance < withdraw_amount {
		return Err(format!(
			"Insufficient balance: {}. Request {} for withdraw",
			balance, withdraw_amount
		))
	} else if withdraw_request.nonce != expected_nonce {
		return Err(format!(
			"Nonce did not change sequentially. Expected: {}, got {}",
			expected_nonce, withdraw_request.nonce
		))
	}

	is_valid
}

pub(super) fn is_valid_withdraw_confirmation(
	channel_state: &ChannelState,
	received_withdraw: &ReceiveWithdrawConfirmation,
) -> Result<(), String> {
	let expiration =
		match channel_state.our_state.withdraws_pending.get(&received_withdraw.total_withdraw) {
			Some(withdraw_state) => Some(withdraw_state.expiration),
			None =>
				if channel_state.our_state.withdraws_expired.len() > 0 {
					let expiration = channel_state
						.our_state
						.withdraws_expired
						.iter()
						.filter(|candidate| {
							candidate.total_withdraw == received_withdraw.total_withdraw
						})
						.next()
						.map(|state| state.expiration);
					expiration
				} else {
					None
				},
		};

	let expiration = match expiration {
		Some(expiration) => expiration,
		None =>
			return Err(format!(
				"Received withdraw confirmation {} was not found in withdraw states",
				received_withdraw.total_withdraw
			)),
	};

	let expected_nonce = get_next_nonce(&channel_state.partner_state);

	if received_withdraw
		.total_withdraw
		.checked_add(channel_state.our_total_withdraw())
		.is_none()
	{
		return Err(format!(
			"The new total_withdraw {} will cause an overflow",
			received_withdraw.total_withdraw
		))
	} else if channel_state.canonical_identifier != received_withdraw.canonical_identifier {
		return Err(format!("Invalid canonical identifier provided in withdraw request"))
	} else if received_withdraw.participant != channel_state.our_state.address {
		return Err(format!("Invalid participant. It must be our address"))
	} else if received_withdraw.sender != channel_state.partner_state.address {
		return Err(format!("Invalid sender. Request must be sent by the partner"))
	} else if received_withdraw.total_withdraw != channel_state.our_total_withdraw() {
		return Err(format!(
			"Total withdraw confirmation {} does not match our total withdraw {}",
			received_withdraw.total_withdraw,
			channel_state.our_total_withdraw()
		))
	} else if received_withdraw.nonce != expected_nonce {
		return Err(format!(
			"Nonce did not change sequentially. Expected: {}, got {}",
			expected_nonce, received_withdraw.nonce
		))
	} else if expiration != received_withdraw.expiration {
		return Err(format!(
			"Invalid expiration {}, withdraw confirmation \
                            must use the same confirmation as the request \
                            otherwise the signature will not match on-chain",
			received_withdraw.expiration
		))
	}

	is_valid_withdraw_signature(
		received_withdraw.canonical_identifier.clone(),
		received_withdraw.sender,
		received_withdraw.participant,
		received_withdraw.total_withdraw,
		received_withdraw.expiration,
		received_withdraw.signature,
	)
}

pub(super) fn is_valid_action_coop_settle(
	channel_state: &ChannelState,
	total_withdraw: TokenAmount,
) -> Result<(), String> {
	is_valid_total_withdraw(channel_state, total_withdraw, true)?;

	if channel_state.our_state.pending_locks.locks.len() > 0 {
		return Err(format!("Coop-Settle not allowed: we still have pending locks"))
	}
	if channel_state.partner_state.pending_locks.locks.len() > 0 {
		return Err(format!("Coop-Settle not allowed: partner still has pending locks"))
	}
	if channel_state.our_state.offchain_total_withdraw() > TokenAmount::zero() {
		return Err(format!("Coop-Settle not allowed: We still have pending withdraws"))
	}
	if channel_state.partner_state.offchain_total_withdraw() > TokenAmount::zero() {
		return Err(format!("Coop-Settle not allowed: partner still has pending withdraws"))
	}

	Ok(())
}

pub(super) fn is_valid_unlock(
	channel_state: &ChannelState,
	sender_state: &mut ChannelEndState,
	unlock: ReceiveUnlock,
) -> Result<PendingLocksState, String> {
	let received_balance_proof = unlock.balance_proof;
	let (_, _, current_transferred_amount, current_locked_amount) =
		get_current_balance_proof(sender_state);
	let lock = match get_lock(sender_state, unlock.secrethash) {
		Some(lock) => lock,
		None =>
			return Err(format!(
				"Invalid unlock message. There is no corresponding lock for {}",
				unlock.secrethash
			)),
	};

	let pending_locks = match compute_locks_without(&mut sender_state.pending_locks, &lock) {
		Some(pending_locks) => pending_locks,
		None =>
			return Err(format!("Invalid unlock message. The lock is unknown {}", unlock.secrethash)),
	};

	let locksroot_without_lock = compute_locksroot(&pending_locks);

	let expected_transferred_amount = current_transferred_amount + lock.amount;
	let expected_locked_amount = current_locked_amount - lock.amount;

	let is_valid_balance_proof =
		is_balance_proof_usable_onchain(&received_balance_proof, channel_state, sender_state);

	if let Err(e) = is_valid_balance_proof {
		return Err(format!("Invalid unlock message. {}", e))
	} else if received_balance_proof.locksroot != locksroot_without_lock {
		// Unlock messages remove a known lock, the new locksroot must have only
		// that lock removed, otherwise the sender may be trying to remove
		// additional locks.
		return Err(format!(
			"Invalid unlock message. \
                            Balance proof's locksroot didn't match. \
                            expected {:?} \
                            got {:?}",
			locksroot_without_lock, received_balance_proof.locksroot
		))
	} else if received_balance_proof.transferred_amount != expected_transferred_amount {
		return Err(format!(
			"Invalid unlock message. \
                            Balance proof's wrong transferred_amount. \
                            expected {} \
                            got {}",
			expected_transferred_amount, received_balance_proof.transferred_amount
		))
	} else if received_balance_proof.locked_amount != expected_locked_amount {
		return Err(format!(
			"Invalid unlock message. \
                            Balance proof's wrong locked_amount. \
                            expected {} \
                            got {}",
			expected_locked_amount, received_balance_proof.locked_amount
		))
	}

	Ok(pending_locks)
}

pub(super) fn is_valid_refund(
	channel_state: &ChannelState,
	refund: ReceiveTransferRefund,
	sender_state: &mut ChannelEndState,
	receiver_state: &ChannelEndState,
	received_transfer: &LockedTransferState,
) -> Result<PendingLocksState, String> {
	let pending_locks = valid_locked_transfer_check(
		channel_state,
		sender_state,
		receiver_state,
		"RefundTransfer",
		&refund.transfer.balance_proof,
		&refund.transfer.lock,
	)?;

	if !refund_transfer_matches_transfer(&refund.transfer, received_transfer) {
		return Err("Refund transfer did not match the received transfer".to_owned())
	}

	Ok(pending_locks)
}

pub(super) fn is_valid_action_withdraw(
	channel_state: &ChannelState,
	withdraw: &ActionChannelWithdraw,
) -> Result<(), String> {
	let balance = views::channel_balance(&channel_state.our_state, &channel_state.partner_state);
	let (_, overflow) = withdraw
		.total_withdraw
		.overflowing_add(channel_state.partner_state.total_withdraw());

	let withdraw_amount = withdraw.total_withdraw - channel_state.our_state.total_withdraw();

	if channel_state.status() != ChannelStatus::Opened {
		return Err("Invalid withdraw, the channel is not opened".to_owned())
	} else if withdraw_amount == TokenAmount::zero() {
		return Err(format!("Total withdraw {:?} did not increase", withdraw.total_withdraw))
	} else if balance < withdraw_amount {
		return Err(format!(
			"Insufficient balance: {:?}. Requested {:?} for withdraw",
			balance, withdraw_amount
		))
	} else if overflow {
		return Err(format!(
			"The new total_withdraw {:?} will cause an overflow",
			withdraw.total_withdraw
		))
	}

	return Ok(())
}

pub(crate) fn refund_transfer_matches_transfer(
	refund_transfer: &LockedTransferState,
	transfer: &LockedTransferState,
) -> bool {
	if let Some(sender) = refund_transfer.balance_proof.sender {
		if sender == transfer.target {
			return false
		}
	}

	transfer.payment_identifier == refund_transfer.payment_identifier &&
		transfer.lock.amount == refund_transfer.lock.amount &&
		transfer.lock.secrethash == refund_transfer.lock.secrethash &&
		transfer.target == refund_transfer.target &&
		transfer.lock.expiration == refund_transfer.lock.expiration &&
		transfer.token == refund_transfer.token
}
