use std::ops::Div;

use super::{
	channel,
	routes,
	utils,
};
use crate::{
	constants::{
		ABSENT_SECRET,
		CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
		DEFAULT_MEDIATION_FEE_MARGIN,
		DEFAULT_WAIT_BEFORE_LOCK_REMOVAL,
		MAX_MEDIATION_FEE_PERC,
		PAYMENT_AMOUNT_BASED_FEE_MARGIN,
	},
	errors::StateTransitionError,
	types::{
		Block,
		BlockNumber,
		ChainState,
		ChannelState,
		ChannelStatus,
		ContractReceiveSecretReveal,
		ErrorInvalidSecretRequest,
		ErrorPaymentSentFailed,
		ErrorRouteFailed,
		ErrorUnlockFailed,
		Event,
		FeeAmount,
		InitiatorTransferState,
		MessageIdentifier,
		PaymentSentSuccess,
		Random,
		ReceiveSecretRequest,
		ReceiveSecretReveal,
		RouteState,
		Secret,
		SecretHash,
		SendLockedTransfer,
		SendMessageEventInner,
		SendSecretReveal,
		StateChange,
		TokenAmount,
		TransferDescriptionWithSecretState,
		TransferState,
		UnlockSuccess,
	},
	views,
};

pub(super) type TransitionResult = std::result::Result<InitiatorTransition, StateTransitionError>;

pub struct InitiatorTransition {
	pub new_state: Option<InitiatorTransferState>,
	pub channel_state: Option<ChannelState>,
	pub events: Vec<Event>,
}

fn calculate_fee_margin(payment_amount: TokenAmount, estimated_fee: FeeAmount) -> FeeAmount {
	if estimated_fee.is_zero() {
		return FeeAmount::zero()
	}

	((estimated_fee * DEFAULT_MEDIATION_FEE_MARGIN.0) / DEFAULT_MEDIATION_FEE_MARGIN.1) +
		((payment_amount * PAYMENT_AMOUNT_BASED_FEE_MARGIN.0) / PAYMENT_AMOUNT_BASED_FEE_MARGIN.1)
}

fn calculate_safe_amount_with_fee(
	payment_amount: TokenAmount,
	estimated_fee: FeeAmount,
) -> TokenAmount {
	payment_amount + estimated_fee + calculate_fee_margin(payment_amount, estimated_fee)
}

fn events_for_unlock_lock(
	initiator_state: &InitiatorTransferState,
	channel_state: &mut ChannelState,
	secret: Secret,
	secrethash: SecretHash,
	pseudo_random_number_generator: &mut Random,
	block_number: BlockNumber,
) -> Result<Vec<Event>, String> {
	let transfer_description = &initiator_state.transfer_description;

	let message_identifier = pseudo_random_number_generator.next();
	let recipient_address = channel_state.partner_state.address;
	let recipient_metadata =
		views::get_address_metadata(recipient_address, vec![initiator_state.route.clone()]);
	let unlock_lock = channel::send_unlock(
		channel_state,
		message_identifier,
		transfer_description.payment_identifier,
		secret.clone(),
		secrethash,
		block_number,
		recipient_metadata,
	)?;

	let payment_sent_success = PaymentSentSuccess {
		secret,
		token_network_registry_address: channel_state.token_network_registry_address,
		token_network_address: channel_state.canonical_identifier.token_network_address,
		identifier: transfer_description.payment_identifier,
		amount: transfer_description.amount,
		target: transfer_description.target,
		route: initiator_state.route.route.clone(),
	};

	let unlock_success =
		UnlockSuccess { identifier: transfer_description.payment_identifier, secrethash };

	Ok(vec![unlock_lock.into(), payment_sent_success.into(), unlock_success.into()])
}

fn send_locked_transfer(
	transfer_description: TransferDescriptionWithSecretState,
	channel_state: ChannelState,
	route_state: RouteState,
	route_states: Vec<RouteState>,
	message_identifier: MessageIdentifier,
	block_number: BlockNumber,
) -> Result<(ChannelState, SendLockedTransfer), String> {
	let lock_expiration = channel::views::get_safe_initial_expiration(
		block_number,
		channel_state.reveal_timeout,
		transfer_description.lock_timeout,
	);
	let total_amount =
		calculate_safe_amount_with_fee(transfer_description.amount, route_state.estimated_fee);
	let recipient_address = channel_state.partner_state.address;
	let recipient_metadata = views::get_address_metadata(recipient_address, route_states.clone());
	let our_address = channel_state.our_state.address;

	channel::send_locked_transfer(
		channel_state,
		transfer_description.initiator,
		transfer_description.target,
		total_amount,
		lock_expiration,
		Some(transfer_description.secret),
		transfer_description.secrethash,
		message_identifier,
		transfer_description.payment_identifier,
		routes::prune_route_table(route_states, route_state, our_address),
		recipient_metadata,
	)
}

pub fn try_new_route(
	mut chain_state: ChainState,
	candidate_route_states: Vec<RouteState>,
	transfer_description: TransferDescriptionWithSecretState,
) -> Result<(Option<InitiatorTransferState>, ChainState, Vec<Event>), String> {
	let mut route_fee_exceeds_max = false;

	let our_address = chain_state.our_address;

	let selected = loop {
		let route_state = match candidate_route_states.iter().next() {
			Some(route_state) => route_state,
			None => break None,
		};

		let next_hop_address = match route_state.hop_after(our_address) {
			Some(next_hop_address) => next_hop_address,
			None => continue,
		};

		let candidate_channel_state = match views::get_channel_by_token_network_and_partner(
			&chain_state,
			transfer_description.token_network_address,
			next_hop_address,
		) {
			Some(channel_state) => channel_state.clone(),
			None => continue,
		};

		let amount_with_fee =
			calculate_safe_amount_with_fee(transfer_description.amount, route_state.estimated_fee);

		let max_amount_limit = transfer_description.amount +
			(transfer_description
				.amount
				.saturating_mul(MAX_MEDIATION_FEE_PERC.0.into())
				.div(MAX_MEDIATION_FEE_PERC.1));
		if amount_with_fee > max_amount_limit {
			route_fee_exceeds_max = true;
			continue
		}

		let is_channel_usable = candidate_channel_state
			.is_usable_for_new_transfer(amount_with_fee, transfer_description.lock_timeout);
		if is_channel_usable {
			break Some((route_state, candidate_channel_state))
		}
	};

	let (initiator_state, events) = if let Some((route_state, channel_state)) = selected {
		let message_identifier = chain_state.pseudo_random_number_generator.next();
		let (channel_state, locked_transfer_event) = send_locked_transfer(
			transfer_description.clone(),
			channel_state,
			route_state.clone(),
			candidate_route_states.clone(),
			message_identifier,
			chain_state.block_number,
		)?;
		let initiator_state = InitiatorTransferState {
			route: route_state.clone(),
			transfer_description,
			channel_identifier: channel_state.canonical_identifier.channel_identifier,
			transfer: locked_transfer_event.transfer.clone(),
			received_secret_request: false,
			transfer_state: TransferState::Pending,
		};
		utils::update_channel(&mut chain_state, channel_state)?;
		(Some(initiator_state), vec![locked_transfer_event.into()])
	} else {
		let mut reason = "None of the available routes could be used".to_owned();
		if route_fee_exceeds_max {
			reason += " and at least one of them exceeded the maximum fee limit";
		}
		let transfer_failed = ErrorPaymentSentFailed {
			token_network_registry_address: transfer_description.token_network_registry_address,
			token_network_address: transfer_description.token_network_address,
			identifier: transfer_description.payment_identifier,
			target: transfer_description.target,
			reason,
		};

		(None, vec![transfer_failed.into()])
	};

	Ok((initiator_state, chain_state, events))
}

fn handle_block(
	mut initiator_state: InitiatorTransferState,
	state_change: Block,
	channel_state: ChannelState,
	pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
	let secrethash = initiator_state.transfer.lock.secrethash;
	let locked_lock = match channel_state.our_state.secrethashes_to_lockedlocks.get(&secrethash) {
		Some(locked_lock) => locked_lock,
		None => {
			if channel_state
				.partner_state
				.secrethashes_to_lockedlocks
				.get(&secrethash)
				.is_some()
			{
				return Ok(InitiatorTransition {
					new_state: Some(initiator_state),
					channel_state: Some(channel_state),
					events: vec![],
				})
			} else {
				return Ok(InitiatorTransition {
					new_state: None,
					channel_state: Some(channel_state),
					events: vec![],
				})
			}
		},
	};

	let lock_expiration_threshold =
		locked_lock.expiration + DEFAULT_WAIT_BEFORE_LOCK_REMOVAL.into();
	let lock_has_expired = channel::validators::is_lock_expired(
		&channel_state.our_state,
		locked_lock,
		state_change.block_number,
		lock_expiration_threshold,
	);

	let mut events: Vec<Event> = vec![];
	let (initiator_state, channel_state) = if lock_has_expired.is_ok() &&
		initiator_state.transfer_state != TransferState::Expired
	{
		let channel_state = if channel_state.status() == ChannelStatus::Opened {
			let recipient_address = channel_state.partner_state.address;
			let recipient_metadata =
				views::get_address_metadata(recipient_address, vec![initiator_state.route.clone()]);
			let locked_lock = locked_lock.clone();
			let (channel_state, expired_lock_events) = channel::send_lock_expired(
				channel_state,
				locked_lock,
				pseudo_random_number_generator,
				recipient_metadata,
			)
			.map_err(Into::into)?;
			events.extend(expired_lock_events.into_iter().map(|event| event.into()));
			channel_state
		} else {
			channel_state
		};

		let reason = if initiator_state.received_secret_request {
			"Lock expired, despite receiving secret request".to_owned()
		} else {
			"Lock expired".to_owned()
		};

		let transfer_description = &initiator_state.transfer_description;
		let payment_identifier = transfer_description.payment_identifier;

		let payment_failed = ErrorPaymentSentFailed {
			token_network_registry_address: transfer_description.token_network_registry_address,
			token_network_address: transfer_description.token_network_address,
			identifier: transfer_description.payment_identifier,
			target: transfer_description.target,
			reason: reason.clone(),
		};
		let route_failed = ErrorRouteFailed {
			secrethash,
			route: initiator_state.route.route.clone(),
			token_network_address: transfer_description.token_network_address,
		};
		let unlock_failed =
			ErrorUnlockFailed { identifier: payment_identifier, secrethash, reason };
		events.extend(vec![payment_failed.into(), route_failed.into(), unlock_failed.into()]);
		initiator_state.transfer_state = TransferState::Expired;

		let lock_exists = channel::lock_exists_in_either_channel_side(&channel_state, secrethash);
		let initiator_state = if lock_exists { Some(initiator_state) } else { None };
		(initiator_state, channel_state)
	} else {
		(Some(initiator_state), channel_state)
	};

	Ok(InitiatorTransition {
		new_state: initiator_state,
		channel_state: Some(channel_state),
		events: vec![],
	})
}

fn handle_receive_secret_request(
	mut initiator_state: InitiatorTransferState,
	state_change: ReceiveSecretRequest,
	channel_state: ChannelState,
	pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
	let is_message_from_target = state_change.sender == initiator_state.transfer_description.target &&
		state_change.secrethash == initiator_state.transfer_description.secrethash &&
		state_change.payment_identifier ==
			initiator_state.transfer_description.payment_identifier;

	if !is_message_from_target {
		return Ok(InitiatorTransition {
			new_state: Some(initiator_state),
			channel_state: Some(channel_state),
			events: vec![],
		})
	}

	let lock = match channel::views::get_lock(
		&channel_state.our_state,
		initiator_state.transfer_description.secrethash,
	) {
		Some(lock) => lock,
		None =>
			return Err(StateTransitionError {
				msg: "Channel does not have the transfer's lock".to_owned(),
			}),
	};

	if initiator_state.received_secret_request {
		return Ok(InitiatorTransition {
			new_state: Some(initiator_state),
			channel_state: Some(channel_state),
			events: vec![],
		})
	}

	let is_valid_secret_request = state_change.amount >=
		initiator_state.transfer_description.amount &&
		state_change.expiration == lock.expiration &&
		initiator_state.transfer_description.secret != ABSENT_SECRET;

	let mut events = vec![];
	if is_valid_secret_request {
		let message_identifier = pseudo_random_number_generator.next();
		let transfer_description = initiator_state.transfer_description.clone();
		let recipient = transfer_description.target;
		let recipient_metadata =
			views::get_address_metadata(recipient, vec![initiator_state.route.clone()]);
		let secret_reveal = SendSecretReveal {
			inner: SendMessageEventInner {
				recipient,
				recipient_metadata,
				canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
				message_identifier,
			},
			secret: transfer_description.secret,
			secrethash: transfer_description.secrethash,
		};
		initiator_state.transfer_state = TransferState::SecretRevealed;
		initiator_state.received_secret_request = true;
		events.push(secret_reveal.into());
	} else {
		initiator_state.received_secret_request = true;
		let invalid_request = ErrorInvalidSecretRequest {
			payment_identifier: state_change.payment_identifier,
			intended_amount: initiator_state.transfer_description.amount,
			actual_amount: state_change.amount,
		};
		events.push(invalid_request.into());
	}

	return Ok(InitiatorTransition {
		new_state: Some(initiator_state),
		channel_state: Some(channel_state),
		events,
	})
}

fn handle_receive_offchain_secret_reveal(
	initiator_state: InitiatorTransferState,
	state_change: ReceiveSecretReveal,
	mut channel_state: ChannelState,
	pseudo_random_number_generator: &mut Random,
	block_number: BlockNumber,
) -> TransitionResult {
	let valid_reveal = state_change.secrethash == initiator_state.transfer_description.secrethash;
	let sent_by_partner = state_change.sender == channel_state.partner_state.address;
	let is_channel_open = channel_state.status() == ChannelStatus::Opened;

	let lock = initiator_state.transfer.lock.clone();
	let expired = channel::validators::is_lock_expired(
		&channel_state.our_state,
		&lock,
		block_number,
		lock.expiration,
	)
	.is_ok();

	let mut events = vec![];
	if valid_reveal && is_channel_open && sent_by_partner && !expired {
		events.extend(
			events_for_unlock_lock(
				&initiator_state,
				&mut channel_state,
				state_change.secret,
				state_change.secrethash,
				pseudo_random_number_generator,
				block_number,
			)
			.map_err(Into::into)?,
		);
	}

	return Ok(InitiatorTransition {
		new_state: Some(initiator_state),
		channel_state: Some(channel_state),
		events,
	})
}

fn handle_receive_onchain_secret_reveal(
	initiator_state: InitiatorTransferState,
	state_change: ContractReceiveSecretReveal,
	mut channel_state: ChannelState,
	pseudo_random_number_generator: &mut Random,
	block_number: BlockNumber,
) -> TransitionResult {
	let secrethash = initiator_state.transfer_description.secrethash;
	let is_valid_secret = state_change.secrethash == secrethash;
	let is_channel_open = channel_state.status() == ChannelStatus::Opened;
	let is_lock_expired = state_change.block_number > initiator_state.transfer.lock.expiration;
	let is_lock_unlocked = is_valid_secret && !is_lock_expired;
	if is_lock_unlocked {
		channel::register_onchain_secret(
			&mut channel_state,
			state_change.secret.clone(),
			state_change.secrethash,
			state_change.block_number,
			true,
		);
	}

	let lock = initiator_state.transfer.lock.clone();
	let expired = channel::validators::is_lock_expired(
		&channel_state.our_state,
		&lock,
		block_number,
		lock.expiration,
	)
	.is_ok();

	let mut events = vec![];
	if is_lock_unlocked && is_channel_open && !expired {
		events.extend(
			events_for_unlock_lock(
				&initiator_state,
				&mut channel_state,
				state_change.secret.clone(),
				secrethash,
				pseudo_random_number_generator,
				block_number,
			)
			.map_err(Into::into)?,
		);
	}

	return Ok(InitiatorTransition {
		new_state: Some(initiator_state),
		channel_state: Some(channel_state),
		events,
	})
}

pub fn state_transition(
	initiator_state: InitiatorTransferState,
	state_change: StateChange,
	channel_state: ChannelState,
	pseudo_random_number_generator: &mut Random,
	block_number: BlockNumber,
) -> TransitionResult {
	match state_change {
		StateChange::Block(inner) =>
			handle_block(initiator_state, inner, channel_state, pseudo_random_number_generator),
		StateChange::ReceiveSecretReveal(inner) => handle_receive_offchain_secret_reveal(
			initiator_state,
			inner,
			channel_state,
			pseudo_random_number_generator,
			block_number,
		),
		StateChange::ReceiveSecretRequest(inner) => handle_receive_secret_request(
			initiator_state,
			inner,
			channel_state,
			pseudo_random_number_generator,
		),
		StateChange::ContractReceiveSecretReveal(inner) => handle_receive_onchain_secret_reveal(
			initiator_state,
			inner,
			channel_state,
			pseudo_random_number_generator,
			block_number,
		),
		_ => Ok(InitiatorTransition {
			new_state: Some(initiator_state),
			channel_state: Some(channel_state),
			events: vec![],
		}),
	}
}
