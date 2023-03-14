use std::iter;

use raiden_primitives::{
	hashing::hash_secret,
	types::{
		Address,
		BlockExpiration,
		BlockHash,
		BlockNumber,
		BlockTimeout,
		CanonicalIdentifier,
		Secret,
		SecretHash,
		TokenAmount,
	},
};

use super::{
	channel,
	routes,
	secret_registry,
	utils::{
		self,
		update_channel,
	},
};
use crate::{
	constants::{
		CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
		PAYEE_STATE_SECRET_KNOWN,
		PAYEE_STATE_TRANSFER_FINAL,
		PAYEE_STATE_TRANSFER_PAID,
		PAYER_STATE_SECRET_KNOWN,
		PAYER_STATE_TRANSFER_FINAL,
		PAYER_STATE_TRANSFER_PAID,
	},
	errors::StateTransitionError,
	types::{
		ActionInitMediator,
		Block,
		ChainState,
		ChannelState,
		ChannelStatus,
		ContractReceiveSecretReveal,
		ErrorUnexpectedReveal,
		ErrorUnlockClaimFailed,
		ErrorUnlockFailed,
		Event,
		HashTimeLockState,
		LockedTransferState,
		MediationPairState,
		MediatorTransferState,
		PayeeState,
		PayerState,
		Random,
		ReceiveLockExpired,
		ReceiveSecretReveal,
		ReceiveTransferRefund,
		ReceiveUnlock,
		SendMessageEventInner,
		SendSecretReveal,
		StateChange,
		UnlockClaimSuccess,
		UnlockSuccess,
		WaitingTransferState,
		WaitingTransferStatus,
	},
	views::{
		self,
		get_addresses_to_channels,
	},
};

pub(super) type TransitionResult = std::result::Result<MediatorTransition, StateTransitionError>;

#[derive(Debug)]
pub struct MediatorTransition {
	pub new_state: Option<MediatorTransferState>,
	pub chain_state: ChainState,
	pub events: Vec<Event>,
}

/// Returns the channel of a given transfer pair or None if it's not found.
fn get_channel(
	chain_state: &ChainState,
	canonical_identifier: CanonicalIdentifier,
) -> Option<&ChannelState> {
	views::get_channel_by_canonical_identifier(chain_state, canonical_identifier)
}

fn is_send_transfer_almost_equal(
	send: &LockedTransferState,
	received: &LockedTransferState,
) -> bool {
	send.payment_identifier == received.payment_identifier &&
		send.token == received.token &&
		send.lock.expiration == received.lock.expiration &&
		send.lock.secrethash == received.lock.secrethash &&
		send.initiator == received.initiator &&
		send.target == received.target
}

pub(super) fn is_safe_to_wait(
	lock_expiration: BlockExpiration,
	reveal_timeout: BlockTimeout,
	block_number: BlockNumber,
) -> Result<(), String> {
	if lock_expiration < reveal_timeout {
		return Err("Lock expiration must be larger than reveal timeout".to_owned())
	}
	let lock_timeout = lock_expiration - block_number;
	if lock_timeout > reveal_timeout {
		return Ok(())
	}

	Err(format!(
		"Lock timeout is unsafe. \
         Timeout must be larger than {} but it is {}.\
         expiration: {} block_number {}",
		reveal_timeout, lock_timeout, lock_expiration, block_number
	))
}

/// Return the amount after fees are taken.
fn get_amount_without_fees(
	_amount_with_fees: TokenAmount,
	channel_in: &ChannelState,
	channel_out: &ChannelState,
) -> Result<Option<TokenAmount>, String> {
	let balance_in = views::channel_balance(&channel_in.our_state, &channel_in.partner_state);
	let _balance_out = views::channel_balance(&channel_out.our_state, &channel_out.partner_state);
	let _receivable =
		channel_in.our_total_deposit() + channel_in.partner_total_deposit() - balance_in;

	if channel_in.fee_schedule.cap_fees != channel_out.fee_schedule.cap_fees {
		return Err(
			"Both channels must have the same cap_fees setting for the same mediator".to_owned()
		)
	}

	// TODO
	// let fee_func = FeeScheduleState::mediation_fee_func()?;
	// let amount_with_fees = find_intersection();
	let amount_with_fees = TokenAmount::zero();

	Ok(Some(amount_with_fees))
}

/// Given a payer transfer tries the given route to proceed with the mediation.
///
/// Args:
///     payer_transfer: The transfer received from the payer_channel.
///     channelidentifiers_to_channels: All the channels available for this
///         transfer.
///
///     pseudo_random_generator: Number generator to generate a message id.
///     block_number: The current block number.
fn forward_transfer_pair(
	chain_state: &mut ChainState,
	payer_transfer: &LockedTransferState,
	payer_channel: ChannelState,
	mut payee_channel: ChannelState,
	block_number: BlockNumber,
) -> Result<(Option<MediationPairState>, Vec<Event>), String> {
	let amount_after_fees = match get_amount_without_fees(
		payer_transfer.lock.amount,
		&payer_channel,
		&payee_channel,
	)? {
		Some(amount) => amount,
		None => return Ok((None, vec![])),
	};

	let lock_timeout = payer_transfer.lock.expiration - block_number;
	let safe_to_use_channel =
		payee_channel.is_usable_for_mediation(amount_after_fees, lock_timeout);
	if !safe_to_use_channel {
		return Ok((None, vec![]))
	}

	if payee_channel.settle_timeout < lock_timeout {
		return Err("Settle timeout must be >= lock timeout".to_owned())
	}

	let message_identifier = chain_state.pseudo_random_number_generator.next();
	let recipient_address = payee_channel.partner_state.address;
	let recipient_metadata =
		views::get_address_metadata(recipient_address, payer_transfer.route_states.clone());
	let (new_payee_channel, locked_transfer_event) = channel::send_locked_transfer(
		payee_channel.clone(),
		payer_transfer.initiator,
		payer_transfer.target,
		amount_after_fees,
		payer_transfer.lock.expiration,
		payer_transfer.secret.clone(),
		payer_transfer.lock.secrethash,
		message_identifier,
		payer_transfer.payment_identifier,
		payer_transfer.route_states.clone(),
		recipient_metadata,
	)?;
	payee_channel = new_payee_channel;
	update_channel(chain_state, payee_channel.clone())?;

	let locked_transfer = locked_transfer_event.transfer.clone();
	let mediated_events = vec![locked_transfer_event.into()];

	let transfer_pair = MediationPairState {
		payer_transfer: payer_transfer.clone(),
		payee_address: payee_channel.partner_state.address,
		payee_transfer: locked_transfer,
		payer_state: PayerState::Pending,
		payee_state: PayeeState::Pending,
	};

	Ok((Some(transfer_pair), mediated_events))
}

/// Try a new route or fail back to a refund.
///
/// The mediator can safely try a new route knowing that the tokens from
/// payer_transfer will cover the expenses of the mediation. If there is no
/// route available that may be used at the moment of the call the mediator may
/// send a refund back to the payer, allowing the payer to try a different
/// route.
fn mediate_transfer(
	mut chain_state: ChainState,
	mut mediator_state: MediatorTransferState,
	payer_channel: &ChannelState,
	payer_transfer: LockedTransferState,
	block_number: BlockNumber,
) -> TransitionResult {
	if Some(payer_channel.partner_state.address) != payer_transfer.balance_proof.sender {
		return Err(StateTransitionError { msg: "Transfer must be signed by sender".to_owned() })
	}

	let our_address = payer_channel.our_state.address;
	// Makes sure we filter routes that have already been used.
	//
	// So in a setup like this, we want to make sure that node 2, having tried to
	// route the transfer through 3 will also try 5 before sending it backwards to 1
	//
	// 1 -> 2 -> 3 -> 4
	//      v         ^
	//      5 -> 6 -> 7
	let candidate_route_states = routes::filter_acceptable_routes(
		mediator_state.routes.clone(),
		mediator_state.refunded_channels.clone(),
		get_addresses_to_channels(&chain_state),
		payer_channel.canonical_identifier.token_network_address,
		our_address,
	);

	let default_token_network_address =
		payer_channel.canonical_identifier.token_network_address.clone();
	for route_state in candidate_route_states {
		let next_hop = match route_state.hop_after(our_address) {
			Some(next_hop) => next_hop,
			None => continue,
		};
		let target_token_network =
			route_state.swaps.get(&our_address).unwrap_or(&default_token_network_address);
		let payee_channel = match views::get_channel_by_token_network_and_partner(
			&chain_state,
			*target_token_network,
			next_hop,
		) {
			Some(channel) => channel.clone(),
			None => continue,
		};

		let (mediation_transfer_pair, mediation_events) = forward_transfer_pair(
			&mut chain_state,
			&payer_transfer,
			payer_channel.clone(),
			payee_channel,
			block_number,
		)
		.map_err(Into::into)?;
		if let Some(mediation_transfer_pair) = mediation_transfer_pair {
			mediator_state.transfers_pair.push(mediation_transfer_pair);
			return Ok(MediatorTransition {
				new_state: Some(mediator_state),
				chain_state,
				events: mediation_events,
			})
		}
	}

	mediator_state.waiting_transfer = Some(WaitingTransferState {
		transfer: payer_transfer,
		status: WaitingTransferStatus::Waiting,
	});
	Ok(MediatorTransition { new_state: Some(mediator_state), chain_state, events: vec![] })
}

/// If it's known the secret is registered on-chain, the node should not send
/// a new transaction. Note there is a race condition:
///
/// - Node B learns the secret on-chain, sends a secret reveal to A
/// - Node A receives the secret reveal off-chain prior to the event for the secret registration, if
///   the lock is in the danger zone A will try to register the secret on-chain, because from its
///   perspective the secret is not there yet.
fn has_secret_registration_started(
	channel_states: Vec<&ChannelState>,
	transfers_pair: &Vec<MediationPairState>,
	secrethash: SecretHash,
) -> bool {
	let is_secret_registered_onchain = channel_states
		.iter()
		.any(|channel_state| channel_state.partner_state.secret_known_onchain(secrethash));
	let has_pending_transaction = transfers_pair
		.iter()
		.any(|pair| pair.payer_state == PayerState::WaitingSecretReveal);

	is_secret_registered_onchain || has_pending_transaction
}

fn events_to_remove_expired_locks(
	chain_state: &mut ChainState,
	mediator_state: &mut MediatorTransferState,
	block_number: BlockNumber,
) -> Result<Vec<Event>, String> {
	let mut events = vec![];

	if mediator_state.transfers_pair.len() == 0 {
		return Ok(events)
	}

	let initial_payer_transfer = mediator_state.transfers_pair[0].payer_transfer.clone();
	for transfer_pair in mediator_state.transfers_pair.iter_mut() {
		let balance_proof = &transfer_pair.payee_transfer.balance_proof;
		let channel_identifier = balance_proof.canonical_identifier.clone();
		let channel_state =
			match views::get_channel_by_canonical_identifier(chain_state, channel_identifier) {
				Some(channel_state) => channel_state.clone(),
				None => return Ok(events),
			};

		let secrethash = mediator_state.secrethash;
		let mut lock: Option<HashTimeLockState> = None;
		if let Some(locked_lock) =
			channel_state.our_state.secrethashes_to_lockedlocks.get(&secrethash)
		{
			if !channel_state.our_state.secrethashes_to_unlockedlocks.contains_key(&secrethash) {
				lock = Some(locked_lock.clone());
			}
		} else if let Some(unlocked_lock) =
			channel_state.our_state.secrethashes_to_unlockedlocks.get(&secrethash)
		{
			lock = Some(unlocked_lock.lock.clone());
		}

		if let Some(lock) = lock {
			let lock_expiration_threshold =
				channel::views::get_sender_expiration_threshold(lock.expiration);
			let has_lock_expired = channel::validators::is_lock_expired(
				&channel_state.our_state,
				&lock,
				block_number,
				lock_expiration_threshold,
			)
			.is_ok();
			let is_channel_open = channel_state.status() == ChannelStatus::Opened;
			let payee_address_metadata = views::get_address_metadata(
				transfer_pair.payee_address,
				initial_payer_transfer.route_states.clone(),
			);

			if has_lock_expired && is_channel_open {
				transfer_pair.payee_state = PayeeState::Expired;
				let (channel_state, expired_lock_events) = channel::send_lock_expired(
					channel_state,
					lock,
					&mut chain_state.pseudo_random_number_generator,
					payee_address_metadata,
				)?;
				utils::update_channel(chain_state, channel_state)?;
				events.extend(
					expired_lock_events.into_iter().map(|event| Event::SendLockExpired(event)),
				);
				events.push(
					ErrorUnlockFailed {
						identifier: transfer_pair.payee_transfer.payment_identifier,
						secrethash,
						reason: "Lock expired".to_owned(),
					}
					.into(),
				)
			}
		}
	}

	Ok(events)
}

/// Reveal the secret off-chain.
///
/// The secret is revealed off-chain even if there is a pending transaction to
/// reveal it on-chain, this allows the unlock to happen off-chain, which is
/// faster.
///
/// This node is named N, suppose there is a mediated transfer with two refund
/// transfers, one from B and one from C:
///
///     A-N-B...B-N-C..C-N-D
///
/// Under normal operation N will first learn the secret from D, then reveal to
/// C, wait for C to inform the secret is known before revealing it to B, and
/// again wait for B before revealing the secret to A.
///
/// If B somehow sent a reveal secret before C and D, then the secret will be
/// revealed to A, but not C and D, meaning the secret won't be propagated
/// forward. Even if D sent a reveal secret at about the same time, the secret
/// will only be revealed to B upon confirmation from C.
///
/// If the proof doesn't arrive in time and the lock's expiration is at risk, N
/// won't lose tokens since it knows the secret can go on-chain at any time.
fn events_for_secret_reveal(
	transfers_pair: &mut Vec<MediationPairState>,
	secret: Secret,
	pseudo_random_number_generator: &mut Random,
) -> Vec<Event> {
	let mut events = vec![];

	for pair in transfers_pair.iter_mut().rev() {
		let payee_knows_secret = PAYEE_STATE_SECRET_KNOWN.contains(&pair.payee_state);
		let payer_knows_secret = PAYER_STATE_SECRET_KNOWN.contains(&pair.payer_state);
		let is_transfer_pending = pair.payer_state == PayerState::Pending;
		let should_send_secret = payee_knows_secret && !payer_knows_secret && is_transfer_pending;

		if should_send_secret {
			let message_identifier = pseudo_random_number_generator.next();
			pair.payer_state = PayerState::SecretRevealed;
			let payer_transfer = &pair.payer_transfer;
			let recipient = payer_transfer.balance_proof.sender.expect("Should be set");
			let reveal_secret = SendSecretReveal {
				inner: SendMessageEventInner {
					recipient,
					recipient_metadata: views::get_address_metadata(
						recipient,
						payer_transfer.route_states.clone(),
					),
					canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
					message_identifier,
				},
				secret: secret.clone(),
				secrethash: SecretHash::from_slice(&hash_secret(&secret.0)),
			};
			events.push(reveal_secret.into());
		}
	}

	events
}

fn events_for_balance_proof(
	chain_state: &mut ChainState,
	transfers_pair: &mut Vec<MediationPairState>,
	secret: Secret,
	secrethash: SecretHash,
) -> Vec<Event> {
	let mut events = vec![];

	for pair in transfers_pair.iter_mut().rev() {
		let payee_knows_secret = PAYEE_STATE_SECRET_KNOWN.contains(&pair.payee_state);
		let payee_paid = PAYEE_STATE_TRANSFER_PAID.contains(&pair.payee_state);

		let mut payee_channel = match get_channel(
			chain_state,
			pair.payee_transfer.balance_proof.canonical_identifier.clone(),
		) {
			Some(payee_channel) => payee_channel.clone(),
			None => continue,
		};
		let payer_channel = match get_channel(
			chain_state,
			pair.payer_transfer.balance_proof.canonical_identifier.clone(),
		) {
			Some(payer_channel) => payer_channel,
			None => continue,
		};

		let payee_channel_open = payee_channel.status() == ChannelStatus::Opened;

		// The mediator must not send to the payee a balance proof if the lock
		// is in the danger zone, because the payer may not do the same and the
		// on-chain unlock may fail. If the lock is nearing it's expiration
		// block, then on-chain unlock should be done, and if successful it can
		// be unlocked off-chain.
		let mut is_safe_to_send_balance_proof = false;
		if is_safe_to_wait(
			pair.payer_transfer.lock.expiration,
			payer_channel.reveal_timeout,
			chain_state.block_number,
		)
		.is_ok()
		{
			is_safe_to_send_balance_proof = true;
		}

		let should_send_balance_proof_to_payee = payee_channel_open &&
			payee_knows_secret &&
			!payee_paid && is_safe_to_send_balance_proof;

		if should_send_balance_proof_to_payee {
			pair.payee_state = PayeeState::BalanceProof;

			let message_identifier = chain_state.pseudo_random_number_generator.next();
			let recipient_address = pair.payee_address;
			let recipient_metadata = views::get_address_metadata(
				recipient_address,
				pair.payee_transfer.route_states.clone(),
			);
			if let Ok(unlock_lock) = channel::send_unlock(
				&mut payee_channel,
				message_identifier,
				pair.payee_transfer.payment_identifier,
				secret.clone(),
				secrethash,
				chain_state.block_number,
				recipient_metadata,
			) {
				let _ = update_channel(chain_state, payee_channel.clone());
				events.push(unlock_lock.into());
				events.push(
					UnlockSuccess {
						identifier: pair.payer_transfer.payment_identifier,
						secrethash: pair.payer_transfer.lock.secrethash,
					}
					.into(),
				)
			}
		}
	}

	events
}

fn events_for_onchain_secretreveal_if_closed(
	chain_state: &ChainState,
	transfers_pair: &mut Vec<MediationPairState>,
	secret: Secret,
	secrethash: SecretHash,
	block_hash: BlockHash,
) -> Vec<Event> {
	let mut events = vec![];

	let mut all_payer_channels = vec![];
	for transfer_pair in transfers_pair.iter() {
		if let Some(channel_state) = get_channel(
			chain_state,
			transfer_pair.payer_transfer.balance_proof.canonical_identifier.clone(),
		) {
			all_payer_channels.push(channel_state);
		}
	}

	let mut transaction_sent =
		has_secret_registration_started(all_payer_channels, transfers_pair, secrethash);

	let pending_transfers = transfers_pair.iter_mut().filter(|pair| {
		!PAYEE_STATE_TRANSFER_FINAL.contains(&pair.payee_state) ||
			!PAYER_STATE_TRANSFER_FINAL.contains(&pair.payer_state)
	});
	for pending_pair in pending_transfers {
		let payer_channel = match get_channel(
			chain_state,
			pending_pair.payer_transfer.balance_proof.canonical_identifier.clone(),
		) {
			Some(payer_channel) => payer_channel,
			None => continue,
		};

		if payer_channel.status() == ChannelStatus::Closed {
			pending_pair.payer_state = PayerState::WaitingSecretReveal;

			if !transaction_sent {
				if let Some(lock) =
					channel::views::get_lock(&payer_channel.partner_state, secrethash)
				{
					let reveal_events = secret_registry::events_for_onchain_secretreveal(
						payer_channel,
						secret.clone(),
						lock.expiration,
						block_hash,
					);

					events.extend(reveal_events);
					transaction_sent = true;
				}
			}
		}
	}

	events
}

fn events_for_onchain_secretreveal_if_dangerzone(
	chain_state: &ChainState,
	transfers_pair: &mut Vec<MediationPairState>,
	secrethash: SecretHash,
	block_number: BlockNumber,
	block_hash: BlockHash,
) -> Result<Vec<Event>, String> {
	let mut events = vec![];

	let mut all_payer_channels = vec![];
	for transfer_pair in transfers_pair.iter() {
		if let Some(channel_state) = get_channel(
			chain_state,
			transfer_pair.payer_transfer.balance_proof.canonical_identifier.clone(),
		) {
			all_payer_channels.push(channel_state);
		}
	}

	let mut transaction_sent =
		has_secret_registration_started(all_payer_channels, transfers_pair, secrethash);

	// Only consider the transfers which have a pair. This means if we have a
	// waiting transfer and for some reason the node knows the secret, it will
	// not try to register it. Otherwise it would be possible for an attacker to
	// reveal the secret late, just to force the node to send an unnecessary
	// transaction.

	let pending_transfers = transfers_pair.iter_mut().filter(|pair| {
		!PAYEE_STATE_TRANSFER_FINAL.contains(&pair.payee_state) ||
			!PAYER_STATE_TRANSFER_FINAL.contains(&pair.payer_state)
	});
	for pair in pending_transfers {
		let payer_channel = match get_channel(
			chain_state,
			pair.payer_transfer.balance_proof.canonical_identifier.clone(),
		) {
			Some(payer_channel) => payer_channel,
			None => continue,
		};

		let lock = &pair.payer_transfer.lock;
		let safe_to_wait =
			is_safe_to_wait(lock.expiration, payer_channel.reveal_timeout, block_number).is_ok();
		let secret_known =
			payer_channel.partner_state.is_secret_known(pair.payer_transfer.lock.secrethash);

		if !safe_to_wait && secret_known {
			pair.payer_state = PayerState::WaitingSecretReveal;

			if !transaction_sent {
				let secret = match payer_channel.partner_state.get_secret(lock.secrethash) {
					Some(secret) => secret,
					None => return Err("The secret should be known at this point".to_owned()),
				};

				let reveal_events = secret_registry::events_for_onchain_secretreveal(
					payer_channel,
					secret,
					lock.expiration,
					block_hash,
				);

				events.extend(reveal_events);

				transaction_sent = true;
			}
		}
	}

	Ok(events)
}

fn events_for_expired_pairs(
	chain_state: &ChainState,
	transfers_pair: &mut Vec<MediationPairState>,
	waiting_transfer: &mut Option<WaitingTransferState>,
	block_number: BlockNumber,
) -> Vec<Event> {
	let mut events = vec![];

	let pending_transfers = transfers_pair.iter_mut().filter(|pair| {
		!PAYEE_STATE_TRANSFER_FINAL.contains(&pair.payee_state) ||
			!PAYER_STATE_TRANSFER_FINAL.contains(&pair.payer_state)
	});
	for pair in pending_transfers {
		let payer_channel = match get_channel(
			chain_state,
			pair.payer_transfer.balance_proof.canonical_identifier.clone(),
		) {
			Some(payer_channel) => payer_channel,
			None => continue,
		};
		let has_payer_transfer_expired = channel::validators::is_transfer_expired(
			&pair.payer_transfer,
			&payer_channel,
			block_number,
		);

		if has_payer_transfer_expired {
			pair.payer_state = PayerState::Expired;
			let unlock_claim_failed = ErrorUnlockClaimFailed {
				identifier: pair.payer_transfer.payment_identifier,
				secrethash: pair.payer_transfer.lock.secrethash,
				reason: "Lock expired".to_owned(),
			};
			events.push(unlock_claim_failed.into());
		}
	}

	if let Some(waiting_transfer) = waiting_transfer {
		let expiration_threshold = channel::views::get_receiver_expiration_threshold(
			waiting_transfer.transfer.lock.expiration,
		);
		let should_waiting_transfer_expire = waiting_transfer.status !=
			WaitingTransferStatus::Expired &&
			expiration_threshold <= block_number;
		if should_waiting_transfer_expire {
			waiting_transfer.status = WaitingTransferStatus::Expired;

			let unlock_claim_failed = ErrorUnlockClaimFailed {
				identifier: waiting_transfer.transfer.payment_identifier,
				secrethash: waiting_transfer.transfer.lock.secrethash,
				reason: "Lock expired".to_owned(),
			};
			events.push(unlock_claim_failed.into());
		}
	}

	events
}

fn set_offchain_secret(
	chain_state: &mut ChainState,
	mediator_state: &mut MediatorTransferState,
	secret: Secret,
	secrethash: SecretHash,
) -> Vec<Event> {
	mediator_state.secret = Some(secret.clone());

	for pair in &mediator_state.transfers_pair {
		if let Some(payer_channel) = get_channel(
			&chain_state,
			pair.payer_transfer.balance_proof.canonical_identifier.clone(),
		) {
			let mut payer_channel = payer_channel.clone();
			channel::register_offchain_secret(&mut payer_channel, secret.clone(), secrethash);
			let _ = update_channel(chain_state, payer_channel);
		}
		if let Some(payee_channel) = get_channel(
			&chain_state,
			pair.payee_transfer.balance_proof.canonical_identifier.clone(),
		) {
			let mut payee_channel = payee_channel.clone();
			channel::register_offchain_secret(&mut payee_channel, secret.clone(), secrethash);
			let _ = update_channel(chain_state, payee_channel);
		}
	}
	// The secret should never be revealed if `waiting_transfer` is not None.
	// For this to happen this node must have received a transfer, which it did
	// *not* mediate, and nevertheless the secret was revealed.
	//
	// This can only be possible if the initiator reveals the secret without the
	// target's secret request, or if the node which sent the `waiting_transfer`
	// has sent another transfer which reached the target (meaning someone along
	// the path will lose tokens).
	if let Some(waiting_transfer) = &mediator_state.waiting_transfer {
		if let Some(payer_channel) = get_channel(
			chain_state,
			waiting_transfer.transfer.balance_proof.canonical_identifier.clone(),
		) {
			let mut payer_channel = payer_channel.clone();
			channel::register_offchain_secret(&mut payer_channel, secret, secrethash);
			let _ = update_channel(chain_state, payer_channel);

			let unexpected_reveal = ErrorUnexpectedReveal {
				secrethash,
				reason: "The mediator has a waiting transfer".to_owned(),
			};

			return vec![unexpected_reveal.into()]
		}
	}

	vec![]
}

fn set_offchain_reveal_state(transfers_pair: &mut Vec<MediationPairState>, payee_address: Address) {
	for pair in transfers_pair {
		if pair.payee_address == payee_address {
			pair.payee_state = PayeeState::SecretRevealed;
		}
	}
}

/// Set the secret to all mediated transfers.
/// The secret should have been learned from the secret registry.
fn set_onchain_secret(
	chain_state: &mut ChainState,
	mediator_state: &mut MediatorTransferState,
	secret: Secret,
	secrethash: SecretHash,
	block_number: BlockNumber,
) -> Vec<Event> {
	mediator_state.secret = Some(secret.clone());

	for pair in &mediator_state.transfers_pair {
		if let Some(payer_channel) = get_channel(
			&chain_state,
			pair.payer_transfer.balance_proof.canonical_identifier.clone(),
		) {
			let mut payer_channel = payer_channel.clone();
			channel::register_onchain_secret(
				&mut payer_channel,
				secret.clone(),
				secrethash,
				block_number,
				true,
			);
			let _ = update_channel(chain_state, payer_channel);
		}
		if let Some(payee_channel) = get_channel(
			&chain_state,
			pair.payee_transfer.balance_proof.canonical_identifier.clone(),
		) {
			let mut payee_channel = payee_channel.clone();
			channel::register_onchain_secret(
				&mut payee_channel,
				secret.clone(),
				secrethash,
				block_number,
				true,
			);
			let _ = update_channel(chain_state, payee_channel);
		}
	}

	// Like the off-chain secret reveal, the secret should never be revealed
	// on-chain if there is a waiting transfer.
	if let Some(waiting_transfer) = &mediator_state.waiting_transfer {
		if let Some(payer_channel) = get_channel(
			chain_state,
			waiting_transfer.transfer.balance_proof.canonical_identifier.clone(),
		) {
			let mut payer_channel = payer_channel.clone();
			channel::register_onchain_secret(
				&mut payer_channel,
				secret,
				secrethash,
				block_number,
				true,
			);
			let _ = update_channel(chain_state, payer_channel);

			let unexpected_reveal = ErrorUnexpectedReveal {
				secrethash,
				reason: "The mediator has a waiting transfer".to_owned(),
			};

			return vec![unexpected_reveal.into()]
		}
	}

	vec![]
}

fn secret_learned(
	mut chain_state: ChainState,
	mut mediator_state: MediatorTransferState,
	secret: Secret,
	secrethash: SecretHash,
	payee_address: Address,
) -> TransitionResult {
	let secret_reveal_events =
		set_offchain_secret(&mut chain_state, &mut mediator_state, secret.clone(), secrethash);
	set_offchain_reveal_state(&mut mediator_state.transfers_pair, payee_address);

	let block_hash = chain_state.block_hash;
	let onchain_secret_reveal = events_for_onchain_secretreveal_if_closed(
		&chain_state,
		&mut mediator_state.transfers_pair,
		secret.clone(),
		secrethash,
		block_hash,
	);

	let offchain_secret_reveal = events_for_secret_reveal(
		&mut mediator_state.transfers_pair,
		secret.clone(),
		&mut chain_state.pseudo_random_number_generator,
	);

	let balance_proof = events_for_balance_proof(
		&mut chain_state,
		&mut mediator_state.transfers_pair,
		secret,
		secrethash,
	);

	let mut events = vec![];
	events.extend(secret_reveal_events);
	events.extend(onchain_secret_reveal);
	events.extend(offchain_secret_reveal);
	events.extend(balance_proof);

	Ok(MediatorTransition { new_state: Some(mediator_state), chain_state, events })
}

fn handle_block(
	chain_state: ChainState,
	mediator_state: Option<MediatorTransferState>,
	state_change: Block,
) -> TransitionResult {
	let mediator_state = match mediator_state {
		Some(mediator_state) => mediator_state,
		None =>
			return Err("Block should be accompanied by a valid mediator state".to_owned().into()),
	};
	let mut events = vec![];

	let mut new_mediator_state = mediator_state;
	let mut new_chain_state = chain_state;
	if let Some(waiting_transfer) = new_mediator_state.waiting_transfer.clone() {
		let secrethash = waiting_transfer.transfer.lock.secrethash;
		let payer_channel_identifier =
			waiting_transfer.transfer.balance_proof.canonical_identifier.clone();

		if let Some(payer_channel) = views::get_channel_by_canonical_identifier(
			&new_chain_state.clone(),
			payer_channel_identifier,
		) {
			let mediation_attempt = mediate_transfer(
				new_chain_state.clone(),
				new_mediator_state.clone(),
				&payer_channel,
				waiting_transfer.transfer,
				state_change.block_number,
			)?;

			if let Some(mut mediator_state) = mediation_attempt.new_state {
				events.extend(mediation_attempt.events);

				let mediation_happened = events
					.iter()
					.find(|event| {
						if let Event::SendLockedTransfer(e) = event {
							return e.transfer.lock.secrethash == secrethash
						}
						false
					})
					.is_some();
				if mediation_happened {
					mediator_state.waiting_transfer = None;
				}
				new_mediator_state = mediator_state;
				new_chain_state = mediation_attempt.chain_state;
			}
		}
	}

	events.extend(
		events_to_remove_expired_locks(
			&mut new_chain_state,
			&mut new_mediator_state,
			state_change.block_number,
		)
		.map_err(Into::into)?,
	);
	events.extend(
		events_for_onchain_secretreveal_if_dangerzone(
			&new_chain_state,
			&mut new_mediator_state.transfers_pair,
			new_mediator_state.secrethash,
			state_change.block_number,
			state_change.block_hash,
		)
		.map_err(Into::into)?,
	);
	events.extend(events_for_expired_pairs(
		&new_chain_state,
		&mut new_mediator_state.transfers_pair,
		&mut new_mediator_state.waiting_transfer,
		state_change.block_number,
	));

	Ok(MediatorTransition {
		new_state: Some(new_mediator_state),
		chain_state: new_chain_state,
		events,
	})
}

fn handle_init(mut chain_state: ChainState, state_change: ActionInitMediator) -> TransitionResult {
	let from_transfer = state_change.from_transfer;
	let mut payer_channel =
		match get_channel(&chain_state, from_transfer.balance_proof.canonical_identifier.clone()) {
			Some(channel) => channel.clone(),
			None => return Ok(MediatorTransition { new_state: None, chain_state, events: vec![] }),
		};

	let mediator_state = MediatorTransferState {
		secrethash: from_transfer.lock.secrethash,
		routes: state_change.candidate_route_states,
		refunded_channels: vec![],
		secret: None,
		transfers_pair: vec![],
		waiting_transfer: None,
	};

	let mut events = vec![];
	let payer_address_metadata = match from_transfer.balance_proof.sender {
		Some(sender) => views::get_address_metadata(sender, from_transfer.route_states.clone()),
		None => None,
	};
	if let Ok(locked_transfer_event) = channel::handle_receive_locked_transfer(
		&mut payer_channel,
		from_transfer.clone(),
		payer_address_metadata,
	) {
		utils::update_channel(&mut chain_state, payer_channel.clone()).map_err(Into::into)?;
		events.push(locked_transfer_event);
	} else {
		return Ok(MediatorTransition { new_state: None, chain_state, events: vec![] })
	}

	let block_number = chain_state.block_number;
	let iteration =
		mediate_transfer(chain_state, mediator_state, &payer_channel, from_transfer, block_number)?;
	events.extend(iteration.events);

	Ok(MediatorTransition {
		new_state: iteration.new_state,
		chain_state: iteration.chain_state,
		events,
	})
}

/// Validate and handle a ReceiveTransferRefund mediator_state change.
/// A node might participate in mediated transfer more than once because of
/// refund transfers, e.g. A-B-C-B-D-T, B tried to mediate the transfer through
/// C, which didn't have an available route to proceed and refunds B, at this
/// point B is part of the path again and will try a new partner to proceed
/// with the mediation through D, D finally reaches the target T.
/// In the above scenario B has two pairs of payer and payee transfers:
///     payer:A payee:C from the first SendLockedTransfer
///     payer:C payee:D from the following SendRefundTransfer
/// Args:
///     mediator_state: Current mediator_state.
///     mediator_state_change: The mediator_state change.
/// Returns:
///     TransitionResult: The resulting iteration.
fn handle_refund_transfer(
	mut chain_state: ChainState,
	mediator_state: Option<MediatorTransferState>,
	state_change: ReceiveTransferRefund,
) -> TransitionResult {
	let mut mediator_state = match mediator_state {
		Some(mediator_state) => mediator_state,
		None =>
			return Err("ReceiveTransferRefund should be accompanied by a valid mediator state"
				.to_owned()
				.into()),
	};

	if mediator_state.secret.is_none() {
		return Ok(MediatorTransition {
			new_state: Some(mediator_state),
			chain_state,
			events: vec![],
		})
	}

	if mediator_state.transfers_pair.len() == 0 {
		return Ok(MediatorTransition {
			new_state: Some(mediator_state),
			chain_state,
			events: vec![],
		})
	}

	// The last sent transfer is the only one that may be refunded, all the
	// previous ones are refunded already.
	let transfer_pair = mediator_state.transfers_pair.last().expect("Checked above");
	let payee_transfer = transfer_pair.payee_transfer.clone();
	let payer_transfer = transfer_pair.payer_transfer.clone();
	let canonical_identifier = payer_transfer.balance_proof.canonical_identifier.clone();
	let mut payer_channel = match get_channel(&chain_state, canonical_identifier) {
		Some(channel) => channel.clone(),
		None =>
			return Ok(MediatorTransition {
				new_state: Some(mediator_state),
				chain_state,
				events: vec![],
			}),
	};

	let refund_transfer_event =
		match channel::handle_refund_transfer(&mut payer_channel, payee_transfer, state_change) {
			Ok(event) => event,
			Err(_) =>
				return Ok(MediatorTransition {
					new_state: Some(mediator_state),
					chain_state,
					events: vec![],
				}),
		};

	update_channel(&mut chain_state, payer_channel.clone()).map_err(Into::into)?;
	mediator_state
		.refunded_channels
		.push(payer_channel.canonical_identifier.channel_identifier);

	let block_number = chain_state.block_number;
	let iteration = mediate_transfer(
		chain_state,
		mediator_state,
		&payer_channel,
		payer_transfer,
		block_number,
	)?;

	let mut events = vec![refund_transfer_event];
	events.extend(iteration.events);

	Ok(MediatorTransition {
		new_state: iteration.new_state,
		chain_state: iteration.chain_state,
		events,
	})
}

fn handle_offchain_secret_reveal(
	chain_state: ChainState,
	mediator_state: Option<MediatorTransferState>,
	state_change: ReceiveSecretReveal,
) -> TransitionResult {
	let mediator_state = match mediator_state {
		Some(mediator_state) => mediator_state,
		None =>
			return Err("ReceiveSecretReveal should be accompanied by a valid mediator state"
				.to_owned()
				.into()),
	};

	let is_valid = utils::is_valid_secret_reveal(&state_change, mediator_state.secrethash);
	let is_secret_known = mediator_state.secret.is_none();

	if mediator_state.transfers_pair.is_empty() {
		// This will not happen during normal operation, but attackers might
		// send weird messages.
		return Ok(MediatorTransition {
			new_state: Some(mediator_state),
			chain_state,
			events: vec![],
		})
	}

	// a SecretReveal should be rejected if the payer transfer
	// has expired. To check for this, we use the last
	// transfer pair.
	let transfer_pair = mediator_state.transfers_pair.last().expect("Should exist");
	let payer_transfer = &transfer_pair.payer_transfer;
	let canonical_identifier = payer_transfer.balance_proof.canonical_identifier.clone();
	let payer_channel = match get_channel(&chain_state, canonical_identifier) {
		Some(channel) => channel,
		None =>
			return Ok(MediatorTransition {
				new_state: Some(mediator_state),
				chain_state,
				events: vec![],
			}),
	};

	let has_payer_transfer_expired = channel::validators::is_transfer_expired(
		&payer_transfer,
		payer_channel,
		chain_state.block_number,
	);

	if is_secret_known && is_valid && !has_payer_transfer_expired {
		return secret_learned(
			chain_state,
			mediator_state,
			state_change.secret,
			state_change.secrethash,
			state_change.sender,
		)
	}

	Ok(MediatorTransition { new_state: Some(mediator_state), chain_state, events: vec![] })
}

fn handle_onchain_secret_reveal(
	mut chain_state: ChainState,
	mediator_state: Option<MediatorTransferState>,
	state_change: ContractReceiveSecretReveal,
) -> TransitionResult {
	let mut mediator_state = match mediator_state {
		Some(mediator_state) => mediator_state,
		None =>
			return Err(
				"ContractReceiveSecretReveal should be accompanied by a valid mediator state"
					.to_owned()
					.into(),
			),
	};

	let mut events = vec![];
	if utils::is_valid_onchain_secret_reveal(&state_change, mediator_state.secrethash) {
		let secret = state_change.secret;
		// Compare against the block number at which the event was emitted.
		let block_number = state_change.block_number;

		let secret_reveal = set_onchain_secret(
			&mut chain_state,
			&mut mediator_state,
			secret.clone(),
			state_change.secrethash,
			block_number,
		);
		let balance_proof = events_for_balance_proof(
			&mut chain_state,
			&mut mediator_state.transfers_pair,
			secret,
			state_change.secrethash,
		);

		events.extend(secret_reveal);
		events.extend(balance_proof);
	}

	Ok(MediatorTransition { new_state: Some(mediator_state), chain_state, events })
}

fn handle_unlock(
	mut chain_state: ChainState,
	mediator_state: Option<MediatorTransferState>,
	state_change: ReceiveUnlock,
) -> TransitionResult {
	let mut mediator_state = match mediator_state {
		Some(mediator_state) => mediator_state,
		None =>
			return Err("ReceiveUnlock should be accompanied by a valid mediator state"
				.to_owned()
				.into()),
	};

	let mut events = vec![];
	let balance_proof_sender = state_change
		.balance_proof
		.sender
		.ok_or("Sender should be set".to_owned().into())?;
	let canonical_identifier = state_change.balance_proof.canonical_identifier.clone();

	for mut pair in mediator_state.transfers_pair.iter_mut() {
		if pair.payer_transfer.balance_proof.sender == Some(balance_proof_sender) {
			if let Some(channel_state) = get_channel(&chain_state, canonical_identifier.clone()) {
				let recipient_metadata = views::get_address_metadata(
					balance_proof_sender,
					mediator_state.routes.clone(),
				);
				let mut channel_state = channel_state.clone();
				if let Ok(handle_unlock_events) = channel::handle_unlock(
					&mut channel_state,
					state_change.clone(),
					recipient_metadata,
				) {
					let _ = update_channel(&mut chain_state, channel_state);

					events.push(handle_unlock_events);

					events.push(
						UnlockClaimSuccess {
							identifier: pair.payee_transfer.payment_identifier,
							secrethash: pair.payee_transfer.lock.secrethash,
						}
						.into(),
					);

					pair.payer_state = PayerState::BalanceProof;
				}
			}
		}
	}

	Ok(MediatorTransition { new_state: Some(mediator_state), chain_state, events })
}

fn handle_lock_expired(
	mut chain_state: ChainState,
	mediator_state: Option<MediatorTransferState>,
	state_change: ReceiveLockExpired,
) -> TransitionResult {
	let mut mediator_state = match mediator_state {
		Some(mediator_state) => mediator_state,
		None =>
			return Err("ReceiveLockExpired should be accompanied by a valid mediator state"
				.to_owned()
				.into()),
	};

	let mut events = vec![];

	for transfer_pair in mediator_state.transfers_pair.iter_mut() {
		let balance_proof = &transfer_pair.payer_transfer.balance_proof;
		let mut channel_state =
			match get_channel(&chain_state, balance_proof.canonical_identifier.clone()) {
				Some(channel) => channel.clone(),
				None =>
					return Ok(MediatorTransition {
						new_state: Some(mediator_state),
						chain_state,
						events: vec![],
					}),
			};

		let recipient_address = channel_state.partner_state.address;
		let recipient_metadata =
			views::get_address_metadata(recipient_address, mediator_state.routes.clone());
		let result = channel::handle_receive_lock_expired(
			&mut channel_state,
			state_change.clone(),
			chain_state.block_number,
			recipient_metadata,
		)?;
		events.extend(result.events);
		if let Some(channel_state) = result.new_state {
			if !channel::views::get_lock(&channel_state.partner_state, mediator_state.secrethash)
				.is_none()
			{
				transfer_pair.payer_state = PayerState::Expired;
			}
			update_channel(&mut chain_state, channel_state).map_err(Into::into)?;
		}
	}

	if let Some(ref waiting_transfer) = mediator_state.waiting_transfer {
		if let Some(waiting_channel) = get_channel(
			&chain_state,
			waiting_transfer.transfer.balance_proof.canonical_identifier.clone(),
		) {
			let mut waiting_channel = waiting_channel.clone();
			let recipient_address = waiting_channel.partner_state.address;
			let recipient_metadata =
				views::get_address_metadata(recipient_address, mediator_state.routes.clone());
			let result = channel::handle_receive_lock_expired(
				&mut waiting_channel,
				state_change,
				chain_state.block_number,
				recipient_metadata,
			)?;
			if let Some(waiting_channel_state) = result.new_state {
				update_channel(&mut chain_state, waiting_channel_state).map_err(Into::into)?;
			}
			events.extend(result.events);
		}
	}

	Ok(MediatorTransition { new_state: Some(mediator_state), chain_state, events })
}

pub fn clear_if_finalized(transition: MediatorTransition) -> MediatorTransition {
	let new_state = match transition.new_state {
		Some(ref new_state) => new_state,
		None => return transition,
	};

	let secrethash = new_state.secrethash;
	for pair in &new_state.transfers_pair {
		if let Some(payer_channel) = get_channel(
			&transition.chain_state,
			pair.payer_transfer.balance_proof.canonical_identifier.clone(),
		) {
			if channel::validators::is_lock_pending(&payer_channel.partner_state, secrethash) {
				return transition
			}
		}

		if let Some(payee_channel) = get_channel(
			&transition.chain_state,
			pair.payee_transfer.balance_proof.canonical_identifier.clone(),
		) {
			if channel::validators::is_lock_pending(&payee_channel.our_state, secrethash) {
				return transition
			}
		}

		if let Some(waiting_transfer_state) = &new_state.waiting_transfer {
			let waiting_transfer = &waiting_transfer_state.transfer;
			let waiting_channel_identifier =
				waiting_transfer.balance_proof.canonical_identifier.clone();
			if let Some(waiting_channel) = views::get_channel_by_canonical_identifier(
				&transition.chain_state,
				waiting_channel_identifier,
			) {
				if channel::validators::is_lock_pending(&waiting_channel.partner_state, secrethash)
				{
					return transition
				}
			}
		}
	}

	MediatorTransition {
		new_state: None,
		chain_state: transition.chain_state,
		events: transition.events,
	}
}

fn sanity_check(transition: MediatorTransition) -> TransitionResult {
	let mediator_state = match transition.new_state {
		Some(ref state) => state,
		None => return Ok(transition),
	};

	if mediator_state
		.transfers_pair
		.iter()
		.any(|pair| PAYEE_STATE_TRANSFER_PAID.contains(&pair.payee_state))
	{
		if mediator_state.secret.is_none() {
			return Err("Mediator state must have secret".to_owned().into())
		}
	}
	if mediator_state
		.transfers_pair
		.iter()
		.any(|pair| PAYER_STATE_TRANSFER_PAID.contains(&pair.payer_state))
	{
		if mediator_state.secret.is_none() {
			return Err("Mediator state must have secret".to_owned().into())
		}
	}

	if mediator_state.transfers_pair.len() > 0 {
		let first_pair = &mediator_state.transfers_pair[0];
		if mediator_state.secrethash != first_pair.payer_transfer.lock.secrethash {
			return Err("Secret hash mismatch".to_owned().into())
		}
	}

	for pair in &mediator_state.transfers_pair {
		if !is_send_transfer_almost_equal(&pair.payee_transfer, &pair.payer_transfer) {
			return Err("Payee and payer transfers are too different".to_owned().into())
		}
	}

	if mediator_state.transfers_pair.len() <= 2 {
		return Ok(transition)
	}

	let exclude_last = mediator_state.transfers_pair.split_last().expect("Checked above").1;
	let exclude_first = mediator_state.transfers_pair.split_first().expect("Checked above").1;
	for (original, refund) in iter::zip(exclude_last, exclude_first) {
		if Some(original.payee_address) != refund.payer_transfer.balance_proof.sender {
			return Err("Payee/payer address mismatch".to_owned().into())
		}

		let transfer_sent = &original.payee_transfer;
		let transfer_received = &refund.payer_transfer;

		if !is_send_transfer_almost_equal(&transfer_sent, &transfer_received) {
			return Err("Payee and payer transfers are too different (refund)".to_owned().into())
		}
	}

	if let Some(ref waiting_transfer) = mediator_state.waiting_transfer {
		let last_transfer_pair = mediator_state
			.transfers_pair
			.last()
			.ok_or("No transfer pairs found".to_owned().into())?;

		let transfer_sent = &last_transfer_pair.payee_transfer;
		let transfer_received = &waiting_transfer.transfer;

		if !is_send_transfer_almost_equal(&transfer_sent, &transfer_received) {
			return Err("Payee and payer transfers are too different (waiting transfer)"
				.to_owned()
				.into())
		}
	}

	Ok(transition)
}

pub fn state_transition(
	chain_state: ChainState,
	mediator_state: Option<MediatorTransferState>,
	state_change: StateChange,
) -> TransitionResult {
	let transition_result = match state_change {
		StateChange::Block(inner) => handle_block(chain_state, mediator_state, inner),
		StateChange::ActionInitMediator(inner) => handle_init(chain_state, inner),
		StateChange::ReceiveTransferRefund(inner) =>
			handle_refund_transfer(chain_state, mediator_state, inner),
		StateChange::ReceiveSecretReveal(inner) =>
			handle_offchain_secret_reveal(chain_state, mediator_state, inner),
		StateChange::ContractReceiveSecretReveal(inner) =>
			handle_onchain_secret_reveal(chain_state, mediator_state, inner),
		StateChange::ReceiveUnlock(inner) => handle_unlock(chain_state, mediator_state, inner),
		StateChange::ReceiveLockExpired(inner) =>
			handle_lock_expired(chain_state, mediator_state, inner),
		_ =>
			return Ok(MediatorTransition { new_state: mediator_state, chain_state, events: vec![] }),
	}?;

	let transition_result = sanity_check(transition_result)?;
	Ok(clear_if_finalized(transition_result))
}
