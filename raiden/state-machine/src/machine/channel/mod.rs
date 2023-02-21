use std::{
	cmp::min,
	ops::{
		Div,
		Mul,
	},
};

use web3::types::Address;

use self::{
	utils::{
		compute_locks_with,
		compute_locks_without,
		compute_locksroot,
		hash_balance_data,
	},
	validators::{
		is_lock_expired,
		is_lock_locked,
		is_valid_action_coop_settle,
		is_valid_action_withdraw,
		is_valid_lock_expired,
		is_valid_locked_transfer,
		is_valid_refund,
		is_valid_unlock,
		is_valid_withdraw_confirmation,
		is_valid_withdraw_expired,
		is_valid_withdraw_request,
	},
	views::{
		get_amount_locked,
		get_current_balance_proof,
		get_lock,
		get_max_withdraw_amount,
		get_next_nonce,
		get_safe_initial_expiration,
	},
};
use crate::{
	constants::{
		CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
		CHANNEL_STATES_PRIOR_TO_CLOSE,
		NUM_DISCRETISATION_POINTS,
	},
	errors::StateTransitionError,
	types::{
		ActionChannelClose,
		ActionChannelCoopSettle,
		ActionChannelSetRevealTimeout,
		ActionChannelWithdraw,
		AddressMetadata,
		BalanceProofState,
		Block,
		BlockExpiration,
		BlockHash,
		BlockNumber,
		CanonicalIdentifier,
		ChannelEndState,
		ChannelState,
		ChannelStatus,
		ContractReceiveChannelBatchUnlock,
		ContractReceiveChannelClosed,
		ContractReceiveChannelDeposit,
		ContractReceiveChannelSettled,
		ContractReceiveChannelWithdraw,
		ContractReceiveUpdateTransfer,
		ContractSendChannelBatchUnlock,
		ContractSendChannelClose,
		ContractSendChannelCoopSettle,
		ContractSendChannelSettle,
		ContractSendChannelUpdateTransfer,
		ContractSendChannelWithdraw,
		ContractSendEventInner,
		CoopSettleState,
		ErrorInvalidActionCoopSettle,
		ErrorInvalidActionSetRevealTimeout,
		ErrorInvalidActionWithdraw,
		ErrorInvalidReceivedLockExpired,
		ErrorInvalidReceivedLockedTransfer,
		ErrorInvalidReceivedTransferRefund,
		ErrorInvalidReceivedUnlock,
		ErrorInvalidReceivedWithdrawConfirmation,
		ErrorInvalidReceivedWithdrawExpired,
		ErrorInvalidReceivedWithdrawRequest,
		Event,
		ExpiredWithdrawState,
		FeeAmount,
		FeeScheduleState,
		HashTimeLockState,
		LockedTransferState,
		Locksroot,
		MediationFeeConfig,
		MessageIdentifier,
		PaymentIdentifier,
		PendingLocksState,
		PendingWithdrawState,
		Random,
		ReceiveLockExpired,
		ReceiveTransferRefund,
		ReceiveUnlock,
		ReceiveWithdrawConfirmation,
		ReceiveWithdrawExpired,
		ReceiveWithdrawRequest,
		RouteState,
		Secret,
		SecretHash,
		SendLockExpired,
		SendLockedTransfer,
		SendMessageEventInner,
		SendProcessed,
		SendUnlock,
		SendWithdrawConfirmation,
		SendWithdrawExpired,
		SendWithdrawRequest,
		StateChange,
		TokenAmount,
		TransactionExecutionStatus,
		TransactionResult,
		UnlockPartialProofState,
	},
	views as global_views,
};

pub mod utils;
pub mod validators;
pub mod views;

type TransitionResult = std::result::Result<ChannelTransition, StateTransitionError>;

pub struct ChannelTransition {
	pub new_state: Option<ChannelState>,
	pub events: Vec<Event>,
}

fn create_send_expired_lock(
	sender_end_state: &mut ChannelEndState,
	locked_lock: HashTimeLockState,
	pseudo_random_number_generator: &mut Random,
	canonical_identifier: CanonicalIdentifier,
	recipient: Address,
	recipient_metadata: Option<AddressMetadata>,
) -> Result<(Option<SendLockExpired>, Option<PendingLocksState>), String> {
	let locked_amount = get_amount_locked(&sender_end_state);
	let balance_proof = match &sender_end_state.balance_proof {
		Some(bp) => bp.clone(),
		None => return Ok((None, None)),
	};
	let updated_locked_amount = locked_amount - locked_lock.amount;
	let transferred_amount = balance_proof.transferred_amount;
	let secrethash = locked_lock.secrethash;
	let pending_locks =
		match compute_locks_without(&mut sender_end_state.pending_locks, &locked_lock) {
			Some(locks) => locks,
			None => return Ok((None, None)),
		};

	let nonce = get_next_nonce(&sender_end_state);
	let locksroot = compute_locksroot(&pending_locks);
	let balance_hash = hash_balance_data(transferred_amount, locked_amount, locksroot.clone())?;
	let balance_proof = BalanceProofState {
		nonce,
		transferred_amount,
		locksroot,
		balance_hash,
		canonical_identifier: canonical_identifier.clone(),
		locked_amount: updated_locked_amount,
		message_hash: None,
		signature: None,
		sender: None,
	};
	let send_lock_expired = SendLockExpired {
		inner: SendMessageEventInner {
			recipient,
			recipient_metadata,
			canonical_identifier,
			message_identifier: pseudo_random_number_generator.next(),
		},
		balance_proof,
		secrethash,
	};

	Ok((Some(send_lock_expired), Some(pending_locks)))
}

fn delete_unclaimed_lock(end_state: &mut ChannelEndState, secrethash: SecretHash) {
	if end_state.secrethashes_to_lockedlocks.contains_key(&secrethash) {
		end_state.secrethashes_to_lockedlocks.remove(&secrethash);
	}

	if end_state.secrethashes_to_unlockedlocks.contains_key(&secrethash) {
		end_state.secrethashes_to_unlockedlocks.remove(&secrethash);
	}
}

fn delete_lock(end_state: &mut ChannelEndState, secrethash: SecretHash) {
	delete_unclaimed_lock(end_state, secrethash);

	if end_state.secrethashes_to_onchain_unlockedlocks.contains_key(&secrethash) {
		end_state.secrethashes_to_onchain_unlockedlocks.remove(&secrethash);
	}
}

/// Check if the lock with `secrethash` exists in either our state or the partner's state"""
pub(super) fn lock_exists_in_either_channel_side(
	channel_state: &ChannelState,
	secrethash: SecretHash,
) -> bool {
	let lock_exists = |end_state: &ChannelEndState, secrethash: SecretHash| {
		if end_state.secrethashes_to_lockedlocks.get(&secrethash).is_some() {
			return true
		}
		if end_state.secrethashes_to_unlockedlocks.get(&secrethash).is_some() {
			return true
		}
		if end_state.secrethashes_to_onchain_unlockedlocks.get(&secrethash).is_some() {
			return true
		}
		false
	};
	lock_exists(&channel_state.our_state, secrethash) ||
		lock_exists(&channel_state.partner_state, secrethash)
}

pub(super) fn send_lock_expired(
	mut channel_state: ChannelState,
	locked_lock: HashTimeLockState,
	pseudo_random_number_generator: &mut Random,
	recipient_metadata: Option<AddressMetadata>,
) -> Result<(ChannelState, Vec<SendLockExpired>), String> {
	if channel_state.status() != ChannelStatus::Opened {
		return Ok((channel_state, vec![]))
	}

	let secrethash = locked_lock.secrethash.clone();
	let (send_lock_expired, pending_locks) = create_send_expired_lock(
		&mut channel_state.our_state,
		locked_lock,
		pseudo_random_number_generator,
		channel_state.canonical_identifier.clone(),
		channel_state.partner_state.address,
		recipient_metadata,
	)?;

	let events = if let (Some(send_lock_expired), Some(pending_locks)) =
		(send_lock_expired, pending_locks)
	{
		channel_state.our_state.pending_locks = pending_locks;
		channel_state.our_state.balance_proof = Some(send_lock_expired.balance_proof.clone());
		channel_state.our_state.nonce = send_lock_expired.balance_proof.nonce;

		delete_unclaimed_lock(&mut channel_state.our_state, secrethash);

		vec![send_lock_expired]
	} else {
		vec![]
	};

	Ok((channel_state, events))
}

fn create_unlock(
	channel_state: &mut ChannelState,
	message_identifier: MessageIdentifier,
	payment_identifier: PaymentIdentifier,
	secret: Secret,
	lock: &HashTimeLockState,
	block_number: BlockNumber,
	recipient_metadata: Option<AddressMetadata>,
) -> Result<(SendUnlock, PendingLocksState), String> {
	if channel_state.status() == ChannelStatus::Opened {
		return Err("Channel is not open".to_owned())
	}

	if !validators::is_lock_pending(&channel_state.our_state, lock.secrethash) {
		return Err("Lock expired".to_owned())
	}

	let expired =
		is_lock_expired(&channel_state.our_state, &lock, block_number, lock.expiration).is_ok();
	if expired {
		return Err("Lock expired".to_owned())
	}

	let our_balance_proof = match &channel_state.our_state.balance_proof {
		Some(balance_proof) => balance_proof,
		None => return Err("No transfers exist on our state".to_owned()),
	};

	let transferred_amount = lock.amount + our_balance_proof.transferred_amount;
	let pending_locks =
		match compute_locks_without(&mut channel_state.our_state.pending_locks, &lock) {
			Some(pending_locks) => pending_locks,
			None => return Err("Lock is pending, it must be in the pending locks".to_owned()),
		};

	let locksroot = compute_locksroot(&pending_locks);
	let token_address = channel_state.token_address;
	let recipient = channel_state.partner_state.address;
	let locked_amount = get_amount_locked(&channel_state.our_state) - lock.amount;
	let nonce = get_next_nonce(&channel_state.our_state);
	channel_state.our_state.nonce = nonce;

	let balance_hash = hash_balance_data(transferred_amount, locked_amount, locksroot.clone())?;

	let balance_proof = BalanceProofState {
		nonce,
		transferred_amount,
		locked_amount,
		locksroot,
		balance_hash,
		canonical_identifier: channel_state.canonical_identifier.clone(),
		message_hash: None,
		signature: None,
		sender: None,
	};

	let unlock_lock = SendUnlock {
		inner: SendMessageEventInner {
			recipient,
			recipient_metadata,
			message_identifier,
			canonical_identifier: channel_state.canonical_identifier.clone(),
		},
		payment_identifier,
		token_address,
		balance_proof,
		secret,
		secrethash: lock.secrethash,
	};

	Ok((unlock_lock, pending_locks))
}

pub(super) fn send_unlock(
	channel_state: &mut ChannelState,
	message_identifier: MessageIdentifier,
	payment_identifier: PaymentIdentifier,
	secret: Secret,
	secrethash: SecretHash,
	block_number: BlockNumber,
	recipient_metadata: Option<AddressMetadata>,
) -> Result<SendUnlock, String> {
	let lock = match get_lock(&channel_state.our_state, secrethash) {
		Some(lock) => lock,
		None => return Err("Caller must ensure the lock exists".to_owned()),
	};

	let (unlock, pending_locks) = create_unlock(
		channel_state,
		message_identifier,
		payment_identifier,
		secret,
		&lock,
		block_number,
		recipient_metadata,
	)?;

	channel_state.our_state.balance_proof = Some(unlock.balance_proof.clone());
	channel_state.our_state.pending_locks = pending_locks;

	delete_lock(&mut channel_state.our_state, lock.secrethash);

	Ok(unlock)
}

pub(super) fn handle_unlock(
	channel_state: &mut ChannelState,
	unlock: ReceiveUnlock,
	recipient_metadata: Option<AddressMetadata>,
) -> Result<Event, String> {
	Ok(
		match is_valid_unlock(
			&channel_state.clone(),
			&mut channel_state.partner_state,
			unlock.clone(),
		) {
			Ok(pending_locks) => {
				channel_state.partner_state.balance_proof = Some(unlock.balance_proof.clone());
				channel_state.partner_state.nonce = unlock.balance_proof.nonce;
				channel_state.partner_state.pending_locks = pending_locks;

				delete_lock(&mut channel_state.partner_state, unlock.secrethash);

				SendProcessed {
					inner: SendMessageEventInner {
						recipient: unlock.balance_proof.sender.expect("Should exist"),
						recipient_metadata,
						canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
						message_identifier: unlock.message_identifier,
					},
				}
				.into()
			},
			Err(e) =>
				ErrorInvalidReceivedUnlock { secrethash: unlock.secrethash, reason: e }.into(),
		},
	)
}

fn register_onchain_secret_endstate(
	end_state: &mut ChannelEndState,
	secret: Secret,
	secrethash: SecretHash,
	secret_reveal_block_number: BlockNumber,
	should_delete_lock: bool,
) {
	let mut pending_lock = None;
	if is_lock_locked(end_state, secrethash) {
		pending_lock = end_state.secrethashes_to_lockedlocks.get_mut(&secrethash);
	}

	if let Some(lock) = end_state.secrethashes_to_unlockedlocks.get_mut(&secrethash) {
		pending_lock = Some(&mut lock.lock);
	}

	if let Some(lock) = pending_lock {
		if lock.expiration < secret_reveal_block_number {
			return
		}

		end_state.secrethashes_to_onchain_unlockedlocks.insert(
			secrethash,
			UnlockPartialProofState {
				secret,
				secrethash,
				lock: lock.clone(),
				amount: lock.amount,
				expiration: lock.expiration,
				encoded: lock.encoded.clone(),
			},
		);

		if should_delete_lock {
			delete_lock(end_state, secrethash);
		}
	}
}

pub(super) fn register_onchain_secret(
	channel_state: &mut ChannelState,
	secret: Secret,
	secrethash: SecretHash,
	secret_reveal_block_number: BlockNumber,
	should_delete_lock: bool,
) {
	register_onchain_secret_endstate(
		&mut channel_state.our_state,
		secret.clone(),
		secrethash,
		secret_reveal_block_number,
		should_delete_lock,
	);
	register_onchain_secret_endstate(
		&mut channel_state.partner_state,
		secret,
		secrethash,
		secret_reveal_block_number,
		should_delete_lock,
	);
}

fn create_locked_transfer(
	channel_state: &mut ChannelState,
	initiator: Address,
	target: Address,
	amount: TokenAmount,
	expiration: BlockExpiration,
	secrethash: SecretHash,
	message_identifier: MessageIdentifier,
	payment_identifier: PaymentIdentifier,
	route_states: Vec<RouteState>,
	recipient_metadata: Option<AddressMetadata>,
) -> Result<(SendLockedTransfer, PendingLocksState), String> {
	if amount >
		global_views::channel_distributable(
			&channel_state.our_state,
			&channel_state.partner_state,
		) {
		return Err("Caller must make sure there is enough balance".to_string())
	}

	if channel_state.status() != ChannelStatus::Opened {
		return Err("Caller must make sure the channel is open".to_string())
	}

	let lock = HashTimeLockState::create(amount, expiration, secrethash);
	let pending_locks =
		match compute_locks_with(&mut channel_state.our_state.pending_locks, lock.clone()) {
			Some(pending_locks) => pending_locks,
			None => return Err("Caller must make sure the lock isn't used twice".to_string()),
		};

	let locksroot = compute_locksroot(&pending_locks);

	let transferred_amount = if let Some(our_balance_proof) = &channel_state.our_state.balance_proof
	{
		our_balance_proof.transferred_amount
	} else {
		TokenAmount::zero()
	};

	if transferred_amount.checked_add(amount).is_none() {
		return Err("Caller must make sure the result wont overflow".to_string())
	}

	let token = channel_state.token_address;
	let locked_amount = get_amount_locked(&channel_state.our_state) + amount;
	let nonce = get_next_nonce(&channel_state.our_state);
	let balance_hash = hash_balance_data(amount, locked_amount, locksroot.clone())?;
	let balance_proof = BalanceProofState {
		nonce,
		transferred_amount,
		locked_amount,
		locksroot,
		balance_hash,
		canonical_identifier: channel_state.canonical_identifier.clone(),
		message_hash: None,
		signature: None,
		sender: None,
	};

	let locked_transfer = LockedTransferState {
		payment_identifier,
		token,
		lock,
		initiator,
		target,
		message_identifier,
		balance_proof,
		route_states: route_states.clone(),
	};

	let recipient = channel_state.partner_state.address;
	let recipient_metadata = match recipient_metadata {
		Some(metadata) => Some(metadata),
		None => global_views::get_address_metadata(recipient, route_states),
	};
	let locked_transfer_event = SendLockedTransfer {
		inner: SendMessageEventInner {
			recipient,
			recipient_metadata,
			canonical_identifier: channel_state.canonical_identifier.clone(),
			message_identifier,
		},
		transfer: locked_transfer,
	};

	Ok((locked_transfer_event, pending_locks))
}

pub(super) fn send_locked_transfer(
	mut channel_state: ChannelState,
	initiator: Address,
	target: Address,
	amount: TokenAmount,
	expiration: BlockExpiration,
	secrethash: SecretHash,
	message_identifier: MessageIdentifier,
	payment_identifier: PaymentIdentifier,
	route_states: Vec<RouteState>,
	recipient_metadata: Option<AddressMetadata>,
) -> Result<(ChannelState, SendLockedTransfer), String> {
	let (locked_transfer, pending_locks) = create_locked_transfer(
		&mut channel_state,
		initiator,
		target,
		amount,
		expiration,
		secrethash,
		message_identifier,
		payment_identifier,
		route_states,
		recipient_metadata,
	)?;

	let transfer = locked_transfer.transfer.clone();
	let lock = transfer.lock.clone();
	channel_state.our_state.balance_proof = Some(transfer.balance_proof.clone());
	channel_state.our_state.nonce = transfer.balance_proof.nonce;
	channel_state.our_state.pending_locks = pending_locks.clone();
	channel_state
		.our_state
		.secrethashes_to_lockedlocks
		.insert(lock.secrethash, lock);

	Ok((channel_state, locked_transfer))
}

fn send_expired_withdraws(
	mut channel_state: ChannelState,
	block_number: BlockNumber,
	pseudo_random_number_generator: &mut Random,
) -> Vec<Event> {
	let mut events = vec![];

	let withdraws_pending = channel_state.our_state.withdraws_pending.clone();
	for withdraw_state in withdraws_pending.values() {
		if !withdraw_state.has_expired(block_number) {
			continue
		}

		let nonce = channel_state.our_state.next_nonce();
		channel_state.our_state.nonce = nonce;

		channel_state.our_state.withdraws_expired.push(ExpiredWithdrawState {
			total_withdraw: withdraw_state.total_withdraw,
			expiration: withdraw_state.expiration,
			nonce: withdraw_state.nonce,
			recipient_metadata: withdraw_state.recipient_metadata.clone(),
		});

		channel_state.our_state.withdraws_pending.remove(&withdraw_state.total_withdraw);

		events.push(
			SendWithdrawExpired {
				inner: SendMessageEventInner {
					recipient: channel_state.partner_state.address,
					recipient_metadata: withdraw_state.recipient_metadata.clone(),
					canonical_identifier: channel_state.canonical_identifier.clone(),
					message_identifier: pseudo_random_number_generator.next(),
				},
				participant: channel_state.our_state.address,
				total_withdraw: withdraw_state.total_withdraw,
				nonce: channel_state.our_state.nonce,
				expiration: withdraw_state.expiration,
			}
			.into(),
		);
	}

	events
}

pub(super) fn handle_receive_lock_expired(
	channel_state: &mut ChannelState,
	state_change: ReceiveLockExpired,
	block_number: BlockNumber,
	recipient_metadata: Option<AddressMetadata>,
) -> TransitionResult {
	let sender = match state_change.balance_proof.sender {
		Some(sender) => sender,
		None =>
			return Err(StateTransitionError { msg: "The transfer's sender is None".to_owned() }),
	};
	let validate_pending_locks = is_valid_lock_expired(
		channel_state,
		state_change.clone(),
		&channel_state.partner_state,
		&channel_state.our_state,
		block_number,
	);

	let events = match validate_pending_locks {
		Ok(pending_locks) => {
			let nonce = state_change.balance_proof.nonce;
			channel_state.partner_state.balance_proof = Some(state_change.balance_proof);
			channel_state.partner_state.nonce = nonce;
			channel_state.partner_state.pending_locks = pending_locks;

			delete_unclaimed_lock(&mut channel_state.partner_state, state_change.secrethash);

			let send_processed = SendProcessed {
				inner: SendMessageEventInner {
					recipient: sender,
					recipient_metadata,
					canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
					message_identifier: state_change.message_identifier,
				},
			};
			vec![send_processed.into()]
		},
		Err(e) => {
			let invalid_lock_expired =
				ErrorInvalidReceivedLockExpired { secrethash: state_change.secrethash, reason: e };
			vec![invalid_lock_expired.into()]
		},
	};

	Ok(ChannelTransition { new_state: Some(channel_state.clone()), events })
}

pub(super) fn handle_receive_locked_transfer(
	channel_state: &mut ChannelState,
	mediated_transfer: LockedTransferState,
	recipient_metadata: Option<AddressMetadata>,
) -> Result<Event, String> {
	let sender = mediated_transfer.balance_proof.sender.ok_or("The transfer's sender is None")?;

	match is_valid_locked_transfer(
		&mediated_transfer,
		&channel_state.clone(),
		&mut channel_state.partner_state,
		&channel_state.our_state,
	) {
		Ok(pending_locks) => {
			channel_state.partner_state.balance_proof =
				Some(mediated_transfer.balance_proof.clone());
			channel_state.partner_state.nonce = mediated_transfer.balance_proof.nonce;
			channel_state.partner_state.pending_locks = pending_locks;

			let lock = mediated_transfer.lock;
			channel_state
				.partner_state
				.secrethashes_to_lockedlocks
				.insert(lock.secrethash, lock);

			Ok(SendProcessed {
				inner: SendMessageEventInner {
					recipient: sender,
					recipient_metadata,
					canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
					message_identifier: mediated_transfer.message_identifier,
				},
			}
			.into())
		},
		Err(e) => Ok(ErrorInvalidReceivedLockedTransfer {
			payment_identifier: mediated_transfer.payment_identifier,
			reason: e,
		}
		.into()),
	}
}

pub(super) fn handle_refund_transfer(
	channel_state: &mut ChannelState,
	received_transfer: LockedTransferState,
	refund: ReceiveTransferRefund,
) -> Result<Event, String> {
	let pending_locks = is_valid_refund(
		&channel_state.clone(),
		refund.clone(),
		&mut channel_state.partner_state,
		&channel_state.our_state,
		&received_transfer,
	);
	let event = match pending_locks {
		Ok(pending_locks) => {
			channel_state.partner_state.balance_proof = Some(refund.transfer.balance_proof.clone());
			channel_state.partner_state.nonce = refund.transfer.balance_proof.nonce;
			channel_state.partner_state.pending_locks = pending_locks;

			let lock = refund.transfer.lock;
			channel_state
				.partner_state
				.secrethashes_to_lockedlocks
				.insert(lock.secrethash, lock);

			let recipient_address = channel_state.partner_state.address;
			let recipient_metadata = global_views::get_address_metadata(
				recipient_address,
				received_transfer.route_states.clone(),
			);
			SendProcessed {
				inner: SendMessageEventInner {
					recipient: recipient_address,
					recipient_metadata,
					canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
					message_identifier: refund.transfer.message_identifier,
				},
			}
			.into()
		},
		Err(msg) => ErrorInvalidReceivedTransferRefund {
			payment_identifier: received_transfer.payment_identifier,
			reason: msg,
		}
		.into(),
	};
	Ok(event)
}

fn handle_block(
	mut channel_state: ChannelState,
	state_change: Block,
	block_number: BlockNumber,
	pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
	let mut events = vec![];

	if channel_state.status() == ChannelStatus::Opened {
		let expired_withdraws = send_expired_withdraws(
			channel_state.clone(),
			block_number,
			pseudo_random_number_generator,
		);
		events.extend(expired_withdraws)
	}

	if channel_state.status() == ChannelStatus::Closed {
		let close_transaction = match channel_state.close_transaction {
			Some(ref transaction) => transaction,
			None =>
				return Err(StateTransitionError {
					msg: "Channel is Closed but close_transaction is not set".to_string(),
				}),
		};
		let closed_block_number = match close_transaction.finished_block_number {
			Some(number) => number,
			None =>
				return Err(StateTransitionError {
					msg: "Channel is Closed but close_transaction block number is missing"
						.to_string(),
				}),
		};

		let settlement_end =
			channel_state.settle_timeout.saturating_add(*closed_block_number).into();
		let state_change_block_number: BlockNumber = state_change.block_number;
		if state_change_block_number > settlement_end {
			channel_state.settle_transaction = Some(TransactionExecutionStatus {
				started_block_number: Some(state_change.block_number),
				finished_block_number: None,
				result: None,
			});

			events.push(
				ContractSendChannelSettle {
					inner: ContractSendEventInner {
						triggered_by_blockhash: state_change.block_hash,
					},
					canonical_identifier: channel_state.canonical_identifier.clone(),
				}
				.into(),
			);
		}
	}

	Ok(ChannelTransition { new_state: Some(channel_state), events })
}

fn set_closed(mut channel_state: ChannelState, block_number: BlockNumber) -> ChannelState {
	if channel_state.close_transaction.is_none() {
		channel_state.close_transaction = Some(TransactionExecutionStatus {
			started_block_number: None,
			finished_block_number: Some(block_number),
			result: Some(TransactionResult::Success),
		});
	} else if let Some(ref mut close_transaction) = channel_state.close_transaction {
		if close_transaction.finished_block_number.is_none() {
			close_transaction.finished_block_number = Some(block_number);
			close_transaction.result = Some(TransactionResult::Success);
		}
	}

	channel_state
}

fn handle_channel_closed(
	channel_state: ChannelState,
	state_change: ContractReceiveChannelClosed,
) -> TransitionResult {
	let mut events = vec![];

	let just_closed = state_change.canonical_identifier == channel_state.canonical_identifier &&
		CHANNEL_STATES_PRIOR_TO_CLOSE
			.to_vec()
			.iter()
			.position(|status| status == &channel_state.status())
			.is_some();

	if just_closed {
		let mut channel_state = set_closed(channel_state.clone(), state_change.block_number);

		let balance_proof = match channel_state.partner_state.balance_proof {
			Some(bp) => bp,
			None => return Ok(ChannelTransition { new_state: Some(channel_state), events: vec![] }),
		};
		let call_update = state_change.transaction_from != channel_state.our_state.address &&
			channel_state.update_transaction.is_none();
		if call_update {
			let expiration =
				channel_state.settle_timeout.saturating_add(*state_change.block_number).into();
			let update = ContractSendChannelUpdateTransfer {
				inner: ContractSendEventInner { triggered_by_blockhash: state_change.block_hash },
				balance_proof,
				expiration,
			};
			channel_state.update_transaction = Some(TransactionExecutionStatus {
				started_block_number: Some(state_change.block_number),
				finished_block_number: None,
				result: None,
			});
			events.push(update.into());
		}
	}

	Ok(ChannelTransition { new_state: Some(channel_state), events })
}

fn set_settled(mut channel_state: ChannelState, block_number: BlockNumber) -> ChannelState {
	if channel_state.settle_transaction.is_none() {
		channel_state.settle_transaction = Some(TransactionExecutionStatus {
			started_block_number: None,
			finished_block_number: Some(block_number),
			result: Some(TransactionResult::Success),
		});
	} else if let Some(ref mut settle_transaction) = channel_state.settle_transaction {
		if settle_transaction.finished_block_number.is_none() {
			settle_transaction.finished_block_number = Some(block_number);
			settle_transaction.result = Some(TransactionResult::Success);
		}
	}
	channel_state
}

fn handle_channel_settled(
	mut channel_state: ChannelState,
	state_change: ContractReceiveChannelSettled,
) -> TransitionResult {
	let mut events = vec![];

	if state_change.canonical_identifier == channel_state.canonical_identifier {
		channel_state = set_settled(channel_state.clone(), state_change.block_number);
		let our_locksroot = state_change.our_onchain_locksroot.clone();
		let partner_locksroot = state_change.our_onchain_locksroot.clone();
		let should_clear_channel = our_locksroot == Locksroot::from(vec![]) &&
			partner_locksroot == Locksroot::from(vec![]);

		if should_clear_channel {
			return Ok(ChannelTransition { new_state: None, events })
		}

		channel_state.our_state.onchain_locksroot = our_locksroot;
		channel_state.partner_state.onchain_locksroot = partner_locksroot;

		events.push(
			ContractSendChannelBatchUnlock {
				inner: ContractSendEventInner { triggered_by_blockhash: state_change.block_hash },
				canonical_identifier: channel_state.canonical_identifier.clone(),
				sender: channel_state.partner_state.address,
			}
			.into(),
		);
	}

	Ok(ChannelTransition { new_state: Some(channel_state), events })
}

fn update_contract_balance(end_state: &mut ChannelEndState, contract_balance: TokenAmount) {
	if contract_balance > end_state.contract_balance {
		end_state.contract_balance = contract_balance;
	}
}

/// Returns a list of numbers from start to stop (inclusive).
#[allow(dead_code)]
fn linspace(start: TokenAmount, stop: TokenAmount, num: TokenAmount) -> Vec<TokenAmount> {
	// assert num > 1, "Must generate at least one step"
	// assert start <= stop, "start must be smaller than stop"

	let step = (stop - start) / (num - 1);

	let mut result = vec![];

	let mut i = TokenAmount::zero();
	while i < num {
		result.push(start + i * step);
		i = i + 1;
	}

	result
}

#[allow(dead_code)]
fn calculate_imbalance_fees(
	channel_capacity: TokenAmount,
	proportional_imbalance_fee: TokenAmount,
) -> Option<Vec<(TokenAmount, FeeAmount)>> {
	if proportional_imbalance_fee == TokenAmount::zero() {
		return None
	}

	if channel_capacity == TokenAmount::zero() {
		return None
	}

	let maximum_slope = TokenAmount::from(10); // 0.1
	let (max_imbalance_fee, overflow) =
		channel_capacity.overflowing_mul(proportional_imbalance_fee);

	if overflow {
		// TODO: Should fail?
		return None
	}

	let max_imbalance_fee = max_imbalance_fee / TokenAmount::from(1_000_000);
	// assert proportional_imbalance_fee / 1e6 <= maximum_slope / 2, "Too high imbalance fee"

	// calculate function parameters
	let s = maximum_slope;
	let c = max_imbalance_fee;
	let o = channel_capacity.div(2);
	let b = o.div(s).div(c);
	let b = b.min(TokenAmount::from(10)); // limit exponent to keep numerical stability;
	let a = c / o.pow(b);

	let f = |x: TokenAmount| -> TokenAmount { a * (x - o).pow(b) };

	// calculate discrete function points
	let num_base_points = min(NUM_DISCRETISATION_POINTS.into(), channel_capacity + 1);
	let x_values: Vec<TokenAmount> = linspace(0u64.into(), channel_capacity, num_base_points);
	let y_values: Vec<TokenAmount> = x_values.iter().map(|x| f(*x)).collect();

	Some(x_values.into_iter().zip(y_values).collect())
}

#[allow(dead_code)]
fn update_fee_schedule_after_balance_change(
	channel_state: &mut ChannelState,
	fee_config: MediationFeeConfig,
) {
	let proportional_imbalance_fee =
		fee_config.get_proportional_imbalance_fee(&channel_state.token_address);
	let imbalance_penalty =
		calculate_imbalance_fees(channel_state.capacity(), proportional_imbalance_fee);

	channel_state.fee_schedule = FeeScheduleState {
		cap_fees: channel_state.fee_schedule.cap_fees,
		flat: channel_state.fee_schedule.flat,
		proportional: channel_state.fee_schedule.proportional,
		imbalance_penalty,
	}
}

fn handle_channel_deposit(
	mut channel_state: ChannelState,
	state_change: ContractReceiveChannelDeposit,
) -> TransitionResult {
	let participant_address = state_change.deposit_transaction.participant_address;
	let contract_balance = state_change.deposit_transaction.contract_balance;

	if participant_address == channel_state.our_state.address {
		update_contract_balance(&mut channel_state.our_state, contract_balance);
	} else if participant_address == channel_state.partner_state.address {
		update_contract_balance(&mut channel_state.partner_state, contract_balance);
	}

	//update_fee_schedule_after_balance_change(&mut channel_state, state_change.fee_config);

	Ok(ChannelTransition { new_state: Some(channel_state), events: vec![] })
}

fn handle_channel_withdraw(
	mut channel_state: ChannelState,
	state_change: ContractReceiveChannelWithdraw,
) -> TransitionResult {
	if state_change.participant != channel_state.our_state.address &&
		state_change.participant != channel_state.partner_state.address
	{
		return Ok(ChannelTransition { new_state: Some(channel_state), events: vec![] })
	}

	let end_state: &mut ChannelEndState =
		if state_change.participant == channel_state.our_state.address {
			&mut channel_state.our_state
		} else {
			&mut channel_state.partner_state
		};

	if let Some(_) = end_state.withdraws_pending.get(&state_change.total_withdraw) {
		end_state.withdraws_pending.remove(&state_change.total_withdraw);
	}
	end_state.onchain_total_withdraw = state_change.total_withdraw;

	// update_fee_schedule_after_balance_change(&mut channel_state, state_change.fee_config);

	return Ok(ChannelTransition { new_state: Some(channel_state), events: vec![] })
}

fn handle_channel_batch_unlock(
	mut channel_state: ChannelState,
	state_change: ContractReceiveChannelBatchUnlock,
) -> TransitionResult {
	if channel_state.status() == ChannelStatus::Settled {
		if state_change.sender == channel_state.our_state.address {
			channel_state.our_state.onchain_locksroot = Locksroot::from(vec![]);
		} else if state_change.sender == channel_state.partner_state.address {
			channel_state.partner_state.onchain_locksroot = Locksroot::from(vec![]);
		}

		let no_unlocks_left_to_do = channel_state.our_state.onchain_locksroot ==
			Locksroot::from(vec![]) &&
			channel_state.partner_state.onchain_locksroot == Locksroot::from(vec![]);
		if no_unlocks_left_to_do {
			return Ok(ChannelTransition { new_state: None, events: vec![] })
		}
	}

	return Ok(ChannelTransition { new_state: Some(channel_state), events: vec![] })
}

fn handle_channel_update_transfer(
	mut channel_state: ChannelState,
	state_change: ContractReceiveUpdateTransfer,
	block_number: BlockNumber,
) -> TransitionResult {
	if state_change.canonical_identifier == channel_state.canonical_identifier {
		channel_state.update_transaction = Some(TransactionExecutionStatus {
			started_block_number: None,
			finished_block_number: Some(block_number),
			result: Some(TransactionResult::Success),
		});
	}

	return Ok(ChannelTransition { new_state: Some(channel_state), events: vec![] })
}

/// This will register the secret and set the lock to the unlocked stated.
///
/// Even though the lock is unlock it is *not* claimed. The capacity will
/// increase once the next balance proof is received.
fn register_secret_endstate(
	end_state: &mut ChannelEndState,
	secret: Secret,
	secrethash: SecretHash,
) {
	if is_lock_locked(end_state, secrethash) {
		let pending_lock = match end_state.secrethashes_to_lockedlocks.get(&secrethash) {
			Some(lock) => lock.clone(),
			None => return,
		};

		end_state.secrethashes_to_lockedlocks.remove(&secrethash);

		end_state.secrethashes_to_unlockedlocks.insert(
			secrethash,
			UnlockPartialProofState {
				lock: pending_lock.clone(),
				secret,
				amount: pending_lock.amount,
				expiration: pending_lock.expiration,
				secrethash,
				encoded: pending_lock.encoded,
			},
		);
	}
}

pub(super) fn register_offchain_secret(
	channel_state: &mut ChannelState,
	secret: Secret,
	secrethash: SecretHash,
) {
	register_secret_endstate(&mut channel_state.our_state, secret.clone(), secrethash);
	register_secret_endstate(&mut channel_state.partner_state, secret, secrethash);
}

fn send_withdraw_request(
	channel_state: &mut ChannelState,
	total_withdraw: TokenAmount,
	block_number: BlockNumber,
	pseudo_random_number_generator: &mut Random,
	recipient_metadata: Option<AddressMetadata>,
	coop_settle: bool,
) -> Vec<Event> {
	let good_channel = CHANNEL_STATES_PRIOR_TO_CLOSE
		.to_vec()
		.iter()
		.position(|status| status == &channel_state.status())
		.is_some();

	if !good_channel {
		return vec![]
	}

	let nonce = channel_state.our_state.next_nonce();
	let expiration = get_safe_initial_expiration(block_number, channel_state.reveal_timeout, None);

	let withdraw_state =
		PendingWithdrawState { total_withdraw, expiration, nonce, recipient_metadata };

	channel_state.our_state.nonce = nonce;
	channel_state
		.our_state
		.withdraws_pending
		.insert(withdraw_state.total_withdraw, withdraw_state.clone());

	vec![SendWithdrawRequest {
		inner: SendMessageEventInner {
			recipient: channel_state.partner_state.address,
			recipient_metadata: withdraw_state.recipient_metadata.clone(),
			canonical_identifier: channel_state.canonical_identifier.clone(),
			message_identifier: pseudo_random_number_generator.next(),
		},
		total_withdraw: withdraw_state.total_withdraw,
		participant: channel_state.our_state.address,
		nonce: channel_state.our_state.nonce,
		expiration: withdraw_state.expiration,
		coop_settle,
	}
	.into()]
}

fn events_for_close(
	channel_state: &mut ChannelState,
	block_number: BlockNumber,
	block_hash: BlockHash,
) -> Result<Vec<Event>, String> {
	if !CHANNEL_STATES_PRIOR_TO_CLOSE.contains(&channel_state.status()) {
		return Ok(vec![])
	}

	channel_state.close_transaction = Some(TransactionExecutionStatus {
		started_block_number: Some(block_number),
		finished_block_number: None,
		result: None,
	});

	let balance_proof = match &channel_state.partner_state.balance_proof {
		Some(balance_proof) => {
			if balance_proof.signature.is_none() {
				return Err("Balance proof is not signed".to_owned().into())
			}
			balance_proof
		},
		None => return Err("No Balance proof found".to_owned().into()),
	};

	let close_event = ContractSendChannelClose {
		inner: ContractSendEventInner { triggered_by_blockhash: block_hash },
		canonical_identifier: channel_state.canonical_identifier.clone(),
		balance_proof: balance_proof.clone(),
	};

	Ok(vec![close_event.into()])
}

fn events_for_coop_settle(
	channel_state: &ChannelState,
	coop_settle_state: &mut CoopSettleState,
	block_number: BlockNumber,
	block_hash: BlockHash,
) -> Vec<Event> {
	if let Some(partner_signature_request) = coop_settle_state.partner_signature_request {
		if let Some(partner_signature_confirmation) =
			coop_settle_state.partner_signature_confirmation
		{
			if coop_settle_state.expiration >= block_number - channel_state.reveal_timeout {
				let send_coop_settle = ContractSendChannelCoopSettle {
					inner: ContractSendEventInner { triggered_by_blockhash: block_hash },
					canonical_identifier: channel_state.canonical_identifier.clone(),
					our_total_withdraw: coop_settle_state.total_withdraw_initiator,
					partner_total_withdraw: coop_settle_state.total_withdraw_partner,
					expiration: coop_settle_state.expiration,
					signature_our_withdraw: partner_signature_confirmation,
					signature_partner_withdraw: partner_signature_request,
				};

				coop_settle_state.transaction = Some(TransactionExecutionStatus {
					started_block_number: Some(block_number),
					finished_block_number: None,
					result: None,
				});

				return vec![send_coop_settle.into()]
			}
		}
	}
	vec![]
}

fn handle_action_close(
	mut channel_state: ChannelState,
	state_change: ActionChannelClose,
	block_number: BlockNumber,
	block_hash: BlockHash,
) -> TransitionResult {
	if channel_state.canonical_identifier != state_change.canonical_identifier {
		return Err("Caller must ensure the canonical IDs match".to_owned().into())
	}

	let events =
		events_for_close(&mut channel_state, block_number, block_hash).map_err(Into::into)?;

	Ok(ChannelTransition { new_state: Some(channel_state), events })
}

fn handle_action_withdraw(
	mut channel_state: ChannelState,
	state_change: ActionChannelWithdraw,
	block_number: BlockNumber,
	pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
	let mut events = vec![];
	match is_valid_action_withdraw(&channel_state, &state_change) {
		Ok(_) => {
			events = send_withdraw_request(
				&mut channel_state,
				state_change.total_withdraw,
				block_number,
				pseudo_random_number_generator,
				state_change.recipient_metadata,
				false,
			);
		},
		Err(e) => {
			events.push(
				ErrorInvalidActionWithdraw {
					attemped_withdraw: state_change.total_withdraw,
					reason: e,
				}
				.into(),
			);
		},
	};
	Ok(ChannelTransition { new_state: Some(channel_state), events })
}

fn handle_action_set_channel_reveal_timeout(
	mut channel_state: ChannelState,
	state_change: ActionChannelSetRevealTimeout,
) -> TransitionResult {
	let double_reveal_timeout: BlockNumber = state_change.reveal_timeout.mul(2u64).into();
	let is_valid_reveal_timeout = state_change.reveal_timeout >= 7u64.into() &&
		channel_state.settle_timeout >= double_reveal_timeout;
	if !is_valid_reveal_timeout {
		return Ok(ChannelTransition {
			new_state: Some(channel_state),
			events: vec![ErrorInvalidActionSetRevealTimeout {
				reveal_timeout: state_change.reveal_timeout,
				reason: format!(
					"Settle timeout should be at least twice as large as reveal timeout"
				),
			}
			.into()],
		})
	}

	channel_state.reveal_timeout = state_change.reveal_timeout;
	Ok(ChannelTransition { new_state: Some(channel_state), events: vec![] })
}

fn handle_action_coop_settle(
	mut channel_state: ChannelState,
	state_change: ActionChannelCoopSettle,
	block_number: BlockNumber,
	pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
	let our_max_total_withdraw =
		get_max_withdraw_amount(&channel_state.our_state, &channel_state.partner_state);
	let partner_max_total_withdraw =
		get_max_withdraw_amount(&channel_state.partner_state, &channel_state.our_state);

	let mut events = vec![];
	match is_valid_action_coop_settle(&channel_state, our_max_total_withdraw) {
		Ok(_) => {
			let expiration =
				get_safe_initial_expiration(block_number, channel_state.reveal_timeout, None);
			let coop_settle = CoopSettleState {
				total_withdraw_initiator: our_max_total_withdraw,
				total_withdraw_partner: partner_max_total_withdraw,
				expiration,
				partner_signature_request: None,
				partner_signature_confirmation: None,
				transaction: None,
			};

			channel_state.our_state.initiated_coop_settle = Some(coop_settle);

			let withdraw_request_events = send_withdraw_request(
				&mut channel_state,
				our_max_total_withdraw,
				block_number,
				pseudo_random_number_generator,
				state_change.recipient_metadata,
				false,
			);
			events.extend(withdraw_request_events);
		},
		Err(e) => events.push(
			ErrorInvalidActionCoopSettle { attempted_withdraw: our_max_total_withdraw, reason: e }
				.into(),
		),
	};

	Ok(ChannelTransition { new_state: Some(channel_state), events })
}

fn handle_receive_withdraw_request(
	mut channel_state: ChannelState,
	state_change: ReceiveWithdrawRequest,
	block_number: BlockNumber,
	block_hash: BlockHash,
	pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
	let mut events = vec![];
	if let Err(msg) = is_valid_withdraw_request(&channel_state, &state_change) {
		return Ok(ChannelTransition {
			new_state: Some(channel_state),
			events: vec![ErrorInvalidReceivedWithdrawRequest {
				attemped_withdraw: state_change.total_withdraw,
				reason: msg,
			}
			.into()],
		})
	}

	let withdraw_state = PendingWithdrawState {
		total_withdraw: state_change.total_withdraw,
		expiration: state_change.expiration,
		nonce: state_change.nonce,
		recipient_metadata: state_change.sender_metadata.clone(),
	};
	channel_state
		.partner_state
		.withdraws_pending
		.insert(withdraw_state.total_withdraw, withdraw_state);
	channel_state.partner_state.nonce = state_change.nonce;

	if channel_state.our_state.initiated_coop_settle.is_none() || state_change.coop_settle {
		let partner_max_total_withdraw =
			get_max_withdraw_amount(&channel_state.partner_state, &channel_state.our_state);
		if partner_max_total_withdraw != state_change.total_withdraw {
			return Ok(ChannelTransition {
				new_state: Some(channel_state),
				events: vec![ErrorInvalidReceivedWithdrawRequest {
					attemped_withdraw: state_change.total_withdraw,
					reason: format!(
						"Partner did not withdraw with maximum balance. Should be {}",
						partner_max_total_withdraw
					),
				}
				.into()],
			})
		}

		if channel_state.partner_state.pending_locks.locks.len() > 0 {
			return Ok(ChannelTransition {
				new_state: Some(channel_state),
				events: vec![ErrorInvalidReceivedWithdrawRequest {
					attemped_withdraw: state_change.total_withdraw,
					reason: format!("Partner has pending transfers"),
				}
				.into()],
			})
		}

		if let Some(our_initiated_coop_settle) =
			channel_state.our_state.initiated_coop_settle.clone().as_mut()
		{
			// There is a coop-settle inplace that we initiated
			// and partner is communicating their total withdraw with us
			if our_initiated_coop_settle.expiration != state_change.expiration {
				return Ok(ChannelTransition {
					new_state: Some(channel_state),
					events: vec![ErrorInvalidReceivedWithdrawRequest {
						attemped_withdraw: state_change.total_withdraw,
						reason: format!(
							"Partner requested withdraw while we initiated a coop-settle: \
                                             Partner's withdraw has differing expiration."
						),
					}
					.into()],
				})
			}

			if our_initiated_coop_settle.total_withdraw_partner != state_change.total_withdraw {
				return Ok(ChannelTransition {
                    new_state: Some(channel_state),
                    events: vec![ErrorInvalidReceivedWithdrawRequest {
                        attemped_withdraw: state_change.total_withdraw,
                        reason: format!(
                            "The expected total withdraw of the partner does not match the withdraw request"
                        ),
                    }
                    .into()],
                });
			}

			our_initiated_coop_settle.partner_signature_request = Some(state_change.signature);
			let coop_settle_events = events_for_coop_settle(
				&channel_state,
				our_initiated_coop_settle,
				block_number,
				block_hash,
			);
			channel_state.our_state.initiated_coop_settle = Some(our_initiated_coop_settle.clone());
			events.extend(coop_settle_events);
		} else {
			let our_max_total_withdraw =
				get_max_withdraw_amount(&channel_state.our_state, &channel_state.partner_state);

			if channel_state.our_state.pending_locks.locks.len() > 0 {
				return Ok(ChannelTransition {
					new_state: Some(channel_state),
					events: vec![ErrorInvalidReceivedWithdrawRequest {
						attemped_withdraw: state_change.total_withdraw,
						reason: format!(
							"Partner initiated coop-settle but we have pending transfers"
						),
					}
					.into()],
				})
			}

			let partner_initiated_coop_settle = CoopSettleState {
				total_withdraw_initiator: state_change.total_withdraw,
				total_withdraw_partner: our_max_total_withdraw,
				expiration: state_change.expiration,
				partner_signature_request: Some(state_change.signature),
				partner_signature_confirmation: None,
				transaction: None,
			};
			channel_state.partner_state.initiated_coop_settle = Some(partner_initiated_coop_settle);
			let send_withdraw_request_events = send_withdraw_request(
				&mut channel_state,
				our_max_total_withdraw,
				state_change.expiration,
				pseudo_random_number_generator,
				state_change.sender_metadata.clone(),
				false,
			);
			events.extend(send_withdraw_request_events);
		}
	}

	channel_state.our_state.nonce = get_next_nonce(&channel_state.our_state);
	let send_withdraw = SendWithdrawConfirmation {
		inner: SendMessageEventInner {
			recipient: channel_state.partner_state.address,
			recipient_metadata: state_change.sender_metadata,
			canonical_identifier: channel_state.canonical_identifier.clone(),
			message_identifier: state_change.message_identifier,
		},
		participant: channel_state.partner_state.address,
		total_withdraw: state_change.total_withdraw,
		nonce: channel_state.our_state.nonce,
		expiration: state_change.expiration,
	};
	events.push(send_withdraw.into());

	Ok(ChannelTransition { new_state: Some(channel_state), events })
}

fn handle_receive_withdraw_confirmation(
	mut channel_state: ChannelState,
	state_change: ReceiveWithdrawConfirmation,
	block_number: BlockNumber,
	block_hash: BlockHash,
) -> TransitionResult {
	let is_valid = is_valid_withdraw_confirmation(&channel_state, &state_change);

	let withdraw_state =
		channel_state.our_state.withdraws_pending.get(&state_change.total_withdraw);
	let mut recipient_metadata = None;
	if let Some(withdraw_state) = withdraw_state {
		recipient_metadata = withdraw_state.recipient_metadata.clone();
	}

	let mut events = vec![];
	match is_valid {
		Ok(_) => {
			channel_state.partner_state.nonce = state_change.nonce;
			events.push(
				SendProcessed {
					inner: SendMessageEventInner {
						recipient: channel_state.partner_state.address,
						recipient_metadata,
						canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
						message_identifier: state_change.message_identifier,
					},
				}
				.into(),
			);

			let partner_initiated_coop_settle = &channel_state.partner_state.initiated_coop_settle;
			if let Some(our_initiated_coop_settle) =
				channel_state.our_state.initiated_coop_settle.clone().as_mut()
			{
				if partner_initiated_coop_settle.is_some() {
					return Err(format!("Only one party can initiate a coop settle").into())
				}

				our_initiated_coop_settle.partner_signature_confirmation =
					Some(state_change.signature);

				let coop_settle_events = events_for_coop_settle(
					&channel_state,
					our_initiated_coop_settle,
					block_number,
					block_hash,
				);
				channel_state.our_state.initiated_coop_settle =
					Some(our_initiated_coop_settle.clone());
				events.extend(coop_settle_events);
			} else {
				// Normal withdraw
				// Only send the transaction on-chain if there is enough time for the
				// withdraw transaction to be mined
				if partner_initiated_coop_settle.is_none() {
					if state_change.expiration >= block_number - channel_state.reveal_timeout {
						let withdraw_onchain = ContractSendChannelWithdraw {
							inner: ContractSendEventInner { triggered_by_blockhash: block_hash },
							canonical_identifier: state_change.canonical_identifier.clone(),
							total_withdraw: state_change.total_withdraw,
							expiration: state_change.expiration,
							partner_signature: state_change.signature,
						};

						events.push(withdraw_onchain.into());
					}
				}
			}
		},
		Err(e) => {
			let invalid_withdraw = ErrorInvalidReceivedWithdrawConfirmation {
				attemped_withdraw: state_change.total_withdraw,
				reason: e,
			};

			events.push(invalid_withdraw.into());
		},
	}

	Ok(ChannelTransition { new_state: Some(channel_state), events })
}

fn handle_receive_withdraw_expired(
	mut channel_state: ChannelState,
	state_change: ReceiveWithdrawExpired,
	block_number: BlockNumber,
) -> TransitionResult {
	let mut events = vec![];

	let withdraw_state =
		match channel_state.partner_state.withdraws_pending.get(&state_change.total_withdraw) {
			Some(withdraw_state) => withdraw_state.clone(),
			None =>
				return Ok(ChannelTransition {
					new_state: Some(channel_state),
					events: vec![ErrorInvalidReceivedWithdrawExpired {
						attemped_withdraw: state_change.total_withdraw,
						reason: format!(
                        "Withdraw expired of {} did not correspond to a previous withdraw request",
                        state_change.total_withdraw
                    ),
					}
					.into()],
				}),
		};

	let is_valid =
		is_valid_withdraw_expired(&channel_state, &state_change, &withdraw_state, block_number);

	match is_valid {
		Ok(_) => {
			channel_state
				.partner_state
				.withdraws_pending
				.remove(&state_change.total_withdraw);
			channel_state.partner_state.nonce = state_change.nonce;

			if let Some(coop_settle) = channel_state.partner_state.initiated_coop_settle.as_ref() {
				if coop_settle.total_withdraw_initiator == withdraw_state.total_withdraw &&
					coop_settle.expiration == withdraw_state.expiration
				{
					// We declare the partner's initated coop-settle as expired
					// and remove it from the state.
					// This will be used in the handling of incoming withdraw messages.
					channel_state.partner_state.initiated_coop_settle = None
				}
			}

			events.push(
				SendProcessed {
					inner: SendMessageEventInner {
						recipient: channel_state.partner_state.address,
						recipient_metadata: withdraw_state.recipient_metadata.clone(),
						canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
						message_identifier: state_change.message_identifier,
					},
				}
				.into(),
			)
		},
		Err(e) => {
			events.push(
				ErrorInvalidReceivedWithdrawExpired {
					attemped_withdraw: state_change.total_withdraw,
					reason: e,
				}
				.into(),
			);
		},
	}

	Ok(ChannelTransition { new_state: Some(channel_state), events })
}

/// Some of the checks below are tautologies for the current version of the
/// codebase. However they are kept in there to check the constraints if/when
/// the code changes.
fn sanity_check(transition: ChannelTransition) -> TransitionResult {
	let channel_state = match transition.new_state {
		Some(ref channel_state) => channel_state,
		None => return Ok(transition),
	};
	let partner_state = &channel_state.partner_state;
	let our_state = &channel_state.our_state;

	let mut previous = TokenAmount::zero();
	let coop_settle =
		our_state.initiated_coop_settle.is_some() || partner_state.initiated_coop_settle.is_some();

	for (total_withdraw, withdraw_state) in our_state.withdraws_pending.iter() {
		if !coop_settle {
			if withdraw_state.total_withdraw <= previous {
				return Err(format!("Total withdraws must be ordered").into())
			}

			if total_withdraw != &withdraw_state.total_withdraw {
				return Err(format!("Total withdraw mismatch").into())
			}

			previous = withdraw_state.total_withdraw;
		}
	}

	let our_balance = global_views::channel_balance(&our_state, &partner_state);
	let partner_balance = global_views::channel_balance(&partner_state, &our_state);

	let channel_capacity = channel_state.capacity();
	if our_balance + partner_balance != channel_capacity {
		return Err(format!("The whole deposit of the channel has to be accounted for.").into())
	}

	let our_locked = get_amount_locked(&our_state);
	let partner_locked = get_amount_locked(&partner_state);

	let (our_bp_locksroot, _, _, our_bp_locked_amount) = get_current_balance_proof(&our_state);
	let (partner_bp_locksroot, _, _, partner_bp_locked_amount) =
		get_current_balance_proof(&partner_state);

	let message = "The sum of the lock's amounts, and the value of the balance proof \
                   locked_amount must be equal, otherwise settle will not reserve the \
                   proper amount of tokens."
		.to_owned();

	if partner_locked != partner_bp_locked_amount {
		return Err(message.clone().into())
	}
	if our_locked != our_bp_locked_amount {
		return Err(message.into())
	}

	let our_distributable = global_views::channel_distributable(&our_state, &partner_state);
	let partner_distributable = global_views::channel_distributable(&partner_state, &our_state);

	// Because of overflow checks, it is possible for the distributable amount
	// to be lower than the available balance, therefore the sanity check has to
	// be lower-than instead of equal-to
	if our_distributable + our_locked > our_balance {
		return Err(format!("Distributable + locked must not exceed balance (own)").into())
	}
	if partner_distributable + partner_locked > partner_balance {
		return Err(format!("Distributable + locked must not exceed balance (partner)").into())
	}

	let our_locksroot = compute_locksroot(&our_state.pending_locks);
	let partner_locksroot = compute_locksroot(&partner_state.pending_locks);

	let message = "The balance proof locks root must match the existing locks. \
                   Otherwise it is not possible to prove on-chain that a given lock was pending."
		.to_owned();
	if our_locksroot != our_bp_locksroot {
		return Err(message.clone().into())
	}
	if partner_locksroot != partner_bp_locksroot {
		return Err(message.into())
	}

	let message =
		"The lock mappings and the pending locks must be synchronised. Otherwise there is a bug."
			.to_owned();
	for lock in partner_state.secrethashes_to_lockedlocks.values() {
		if !partner_state.pending_locks.locks.contains(&lock.encoded) {
			return Err(message.clone().into())
		}
	}
	for lock in partner_state.secrethashes_to_unlockedlocks.values() {
		if !partner_state.pending_locks.locks.contains(&lock.encoded) {
			return Err(message.clone().into())
		}
	}
	for lock in partner_state.secrethashes_to_onchain_unlockedlocks.values() {
		if !partner_state.pending_locks.locks.contains(&lock.encoded) {
			return Err(message.clone().into())
		}
	}
	for lock in our_state.secrethashes_to_lockedlocks.values() {
		if !our_state.pending_locks.locks.contains(&lock.encoded) {
			return Err(message.clone().into())
		}
	}
	for lock in our_state.secrethashes_to_unlockedlocks.values() {
		if !our_state.pending_locks.locks.contains(&lock.encoded) {
			return Err(message.clone().into())
		}
	}
	for lock in our_state.secrethashes_to_onchain_unlockedlocks.values() {
		if !our_state.pending_locks.locks.contains(&lock.encoded) {
			return Err(message.clone().into())
		}
	}

	Ok(transition)
}

pub fn state_transition(
	channel_state: ChannelState,
	state_change: StateChange,
	block_number: BlockNumber,
	block_hash: BlockHash,
	pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
	let transition = match state_change {
		StateChange::ActionChannelClose(inner) =>
			handle_action_close(channel_state, inner, block_number, block_hash),
		StateChange::ActionChannelWithdraw(inner) => handle_action_withdraw(
			channel_state,
			inner,
			block_number,
			pseudo_random_number_generator,
		),
		StateChange::ActionChannelCoopSettle(inner) => handle_action_coop_settle(
			channel_state,
			inner,
			block_number,
			pseudo_random_number_generator,
		),
		StateChange::ActionChannelSetRevealTimeout(inner) =>
			handle_action_set_channel_reveal_timeout(channel_state, inner),
		StateChange::Block(inner) =>
			handle_block(channel_state, inner, block_number, pseudo_random_number_generator),
		StateChange::ContractReceiveChannelClosed(inner) =>
			handle_channel_closed(channel_state, inner),
		StateChange::ContractReceiveChannelSettled(inner) =>
			handle_channel_settled(channel_state, inner),
		StateChange::ContractReceiveChannelDeposit(inner) =>
			handle_channel_deposit(channel_state, inner),
		StateChange::ContractReceiveChannelWithdraw(inner) =>
			handle_channel_withdraw(channel_state, inner),
		StateChange::ContractReceiveChannelBatchUnlock(inner) =>
			handle_channel_batch_unlock(channel_state, inner),
		StateChange::ContractReceiveUpdateTransfer(inner) =>
			handle_channel_update_transfer(channel_state, inner, block_number),
		StateChange::ReceiveWithdrawRequest(inner) => handle_receive_withdraw_request(
			channel_state,
			inner,
			block_number,
			block_hash,
			pseudo_random_number_generator,
		),
		StateChange::ReceiveWithdrawConfirmation(inner) =>
			handle_receive_withdraw_confirmation(channel_state, inner, block_number, block_hash),
		StateChange::ReceiveWithdrawExpired(inner) =>
			handle_receive_withdraw_expired(channel_state, inner, block_number),
		_ => Err(StateTransitionError { msg: String::from("Could not transition channel") }),
	}?;

	sanity_check(transition)
}
