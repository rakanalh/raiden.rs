use std::collections::HashMap;

use raiden_primitives::types::SecretHash;

use super::{
	channel,
	initiator,
	routes,
	utils,
};
use crate::{
	errors::StateTransitionError,
	types::{
		ActionCancelPayment,
		ActionInitInitiator,
		ActionTransferReroute,
		Block,
		CanonicalIdentifier,
		ChainState,
		ContractReceiveSecretReveal,
		ErrorPaymentSentFailed,
		ErrorRouteFailed,
		ErrorUnlockClaimFailed,
		ErrorUnlockFailed,
		Event,
		InitiatorPaymentState,
		InitiatorTransferState,
		ReceiveLockExpired,
		ReceiveSecretRequest,
		ReceiveSecretReveal,
		ReceiveTransferCancelRoute,
		RouteState,
		StateChange,
		TransferDescriptionWithSecretState,
		TransferState,
	},
	views::{
		self,
		get_addresses_to_channels,
	},
};

pub(super) type TransitionResult =
	std::result::Result<InitiatorManagerTransition, StateTransitionError>;

#[derive(Debug)]
pub struct InitiatorManagerTransition {
	pub new_state: Option<InitiatorPaymentState>,
	pub chain_state: ChainState,
	pub events: Vec<Event>,
}

fn can_cancel(initiator: &InitiatorTransferState) -> bool {
	initiator.transfer_state != TransferState::Canceled
}

fn transfer_exists(payment_state: &InitiatorPaymentState, secrethash: SecretHash) -> bool {
	payment_state.initiator_transfers.contains_key(&secrethash)
}

fn cancel_other_transfers(payment_state: &mut InitiatorPaymentState) {
	for initiator_state in payment_state.initiator_transfers.values_mut() {
		initiator_state.transfer_state = TransferState::Canceled;
	}
}

fn events_for_cancel_current_route(
	route_state: &RouteState,
	transfer_description: &TransferDescriptionWithSecretState,
) -> Vec<Event> {
	vec![
		ErrorUnlockFailed {
			identifier: transfer_description.payment_identifier,
			secrethash: transfer_description.secrethash,
			reason: "route was canceled".to_string(),
		}
		.into(),
		ErrorRouteFailed {
			secrethash: transfer_description.secrethash,
			route: route_state.route.clone(),
			token_network_address: transfer_description.token_network_address,
		}
		.into(),
	]
}

fn cancel_current_route(
	payment_state: &mut InitiatorPaymentState,
	initiator_state: &InitiatorTransferState,
) -> Vec<Event> {
	payment_state.cancelled_channels.push(initiator_state.channel_identifier);

	events_for_cancel_current_route(&initiator_state.route, &initiator_state.transfer_description)
}

fn subdispatch_to_initiator_transfer(
	mut chain_state: ChainState,
	mut payment_state: InitiatorPaymentState,
	initiator_state: &InitiatorTransferState,
	state_change: StateChange,
) -> TransitionResult {
	let channel_identifier = initiator_state.channel_identifier;
	let channel_state = match views::get_channel_by_canonical_identifier(
		&chain_state,
		CanonicalIdentifier {
			chain_identifier: chain_state.chain_id,
			token_network_address: initiator_state.transfer_description.token_network_address,
			channel_identifier,
		},
	) {
		Some(channel_state) => channel_state,
		None =>
			return Ok(InitiatorManagerTransition {
				new_state: Some(payment_state),
				chain_state,
				events: vec![],
			}),
	};

	let sub_iteration = initiator::state_transition(
		initiator_state.clone(),
		state_change,
		channel_state.clone(),
		&mut chain_state.pseudo_random_number_generator,
		chain_state.block_number,
	)?;

	match sub_iteration.new_state {
		Some(transfer_state) => {
			payment_state
				.initiator_transfers
				.insert(initiator_state.transfer.lock.secrethash, transfer_state);
		},
		None => {
			payment_state
				.initiator_transfers
				.remove(&initiator_state.transfer.lock.secrethash);
		},
	}

	Ok(InitiatorManagerTransition {
		new_state: Some(payment_state),
		chain_state,
		events: sub_iteration.events,
	})
}

fn subdispatch_to_all_initiator_transfer(
	mut payment_state: InitiatorPaymentState,
	mut chain_state: ChainState,
	state_change: StateChange,
) -> TransitionResult {
	let mut events = vec![];

	for initiator_state in payment_state.initiator_transfers.clone().values() {
		let sub_iteration = subdispatch_to_initiator_transfer(
			chain_state.clone(),
			payment_state.clone(),
			initiator_state,
			state_change.clone(),
		)?;
		chain_state = sub_iteration.chain_state;
		payment_state =
			sub_iteration.new_state.expect("Subdispatch returns a correct payment_state");
		events.extend(sub_iteration.events);
	}

	Ok(InitiatorManagerTransition { new_state: Some(payment_state), chain_state, events })
}

pub fn handle_block(
	chain_state: ChainState,
	payment_state: Option<InitiatorPaymentState>,
	state_change: Block,
) -> TransitionResult {
	let payment_state = match payment_state {
		Some(payment_state) => payment_state,
		None =>
			return Err(StateTransitionError {
				msg: "Block state change should be accompanied by a valid payment state".to_owned(),
			}),
	};
	subdispatch_to_all_initiator_transfer(
		payment_state,
		chain_state,
		StateChange::Block(state_change),
	)
}

pub fn handle_init_initiator(
	mut chain_state: ChainState,
	payment_state: Option<InitiatorPaymentState>,
	state_change: ActionInitInitiator,
) -> TransitionResult {
	let mut payment_state = payment_state.clone();
	let mut events = vec![];
	if payment_state.is_none() {
		let (new_state, new_chain_state, iteration_events) = initiator::try_new_route(
			chain_state.clone(),
			state_change.routes.clone(),
			state_change.transfer,
		)
		.map_err(Into::into)?;

		chain_state = new_chain_state;
		events = iteration_events;

		if let Some(new_state) = new_state {
			let mut initiator_transfers = HashMap::new();
			initiator_transfers.insert(new_state.transfer.lock.secrethash, new_state);
			payment_state = Some(InitiatorPaymentState {
				routes: state_change.routes,
				initiator_transfers,
				cancelled_channels: vec![],
			});
		}
	}

	Ok(InitiatorManagerTransition { new_state: payment_state, chain_state, events })
}

pub fn handle_action_cancel_payment(
	chain_state: ChainState,
	payment_state: Option<InitiatorPaymentState>,
	_state_change: ActionCancelPayment,
) -> TransitionResult {
	let mut payment_state = match payment_state {
		Some(payment_state) => payment_state,
		None => {
			return Err(StateTransitionError {
                msg: "ActionCancelPayment state change should be accompanied by a valid payment state".to_owned(),
            });
		},
	};

	let mut events = vec![];
	for initiator_state in payment_state.initiator_transfers.clone().values_mut() {
		let channel_identifier = initiator_state.channel_identifier;
		let channel_state = match views::get_channel_by_canonical_identifier(
			&chain_state,
			CanonicalIdentifier {
				chain_identifier: chain_state.chain_id,
				token_network_address: initiator_state.transfer_description.token_network_address,
				channel_identifier,
			},
		) {
			Some(channel_state) => channel_state,
			None => continue,
		};

		if can_cancel(initiator_state) {
			let transfer_description = initiator_state.transfer_description.clone();
			let mut cancel_events = cancel_current_route(&mut payment_state, initiator_state);

			initiator_state.transfer_state = TransferState::Canceled;

			let cancel = ErrorPaymentSentFailed {
				token_network_registry_address: channel_state.token_network_registry_address,
				token_network_address: channel_state.canonical_identifier.token_network_address,
				identifier: transfer_description.payment_identifier,
				target: transfer_description.target,
				reason: "user canceled payment".to_string(),
			};

			cancel_events.push(cancel.into());
			events.extend(cancel_events);
		}
	}

	Ok(InitiatorManagerTransition { new_state: Some(payment_state), chain_state, events })
}

pub fn handle_transfer_cancel_route(
	chain_state: ChainState,
	payment_state: Option<InitiatorPaymentState>,
	state_change: ReceiveTransferCancelRoute,
) -> TransitionResult {
	let mut payment_state = match payment_state {
		Some(payment_state) => payment_state,
		None => {
			return Err(StateTransitionError {
                msg: "TransferCancelRoute state change should be accompanied by a valid payment state".to_owned(),
            });
		},
	};

	let mut events = vec![];

	if let Some(initiator_state) = payment_state
		.initiator_transfers
		.clone()
		.get(&state_change.transfer.lock.secrethash)
	{
		if can_cancel(initiator_state) {
			let cancel_events = cancel_current_route(&mut payment_state, initiator_state);
			events.extend(cancel_events);
		}
	}

	Ok(InitiatorManagerTransition { new_state: Some(payment_state), chain_state, events })
}

pub fn handle_action_transfer_reroute(
	mut chain_state: ChainState,
	payment_state: Option<InitiatorPaymentState>,
	state_change: ActionTransferReroute,
) -> TransitionResult {
	let mut payment_state = match payment_state {
		Some(payment_state) => payment_state,
		None => {
			return Err(StateTransitionError {
                msg: "TransferCancelRoute state change should be accompanied by a valid payment state".to_owned(),
            });
		},
	};

	let initiator_state =
		match payment_state.initiator_transfers.get(&state_change.transfer.lock.secrethash) {
			Some(initiator_state) => initiator_state,
			None =>
				return Ok(InitiatorManagerTransition {
					new_state: Some(payment_state),
					chain_state,
					events: vec![],
				}),
		};
	let channel_identifier = initiator_state.channel_identifier;
	let mut channel_state = match views::get_channel_by_canonical_identifier(
		&chain_state,
		CanonicalIdentifier {
			chain_identifier: chain_state.chain_id,
			token_network_address: initiator_state.transfer_description.token_network_address,
			channel_identifier,
		},
	) {
		Some(channel_state) => channel_state.clone(),
		None =>
			return Ok(InitiatorManagerTransition {
				new_state: Some(payment_state),
				chain_state,
				events: vec![],
			}),
	};

	let refund_transfer = state_change.transfer;
	let original_transfer = &initiator_state.transfer;

	let is_valid_lock = refund_transfer.lock.secrethash == original_transfer.lock.secrethash &&
		refund_transfer.lock.amount == original_transfer.lock.amount &&
		refund_transfer.lock.expiration == original_transfer.lock.expiration;

	let is_valid_refund =
		channel::validators::refund_transfer_matches_transfer(&refund_transfer, &original_transfer);

	let recipient_address = channel_state.partner_state.address;
	let recipient_metadata =
		views::get_address_metadata(recipient_address, payment_state.routes.clone());
	let received_locked_transfer_result = channel::handle_receive_locked_transfer(
		&mut channel_state,
		refund_transfer,
		recipient_metadata,
	);

	if !is_valid_lock || !is_valid_refund || received_locked_transfer_result.is_err() {
		return Ok(InitiatorManagerTransition {
			new_state: Some(payment_state),
			chain_state,
			events: vec![],
		})
	}

	let mut events = vec![];
	if let Ok(received_locked_transfer_event) = received_locked_transfer_result {
		events.push(received_locked_transfer_event);
	}

	let our_address = channel_state.our_state.address;
	utils::update_channel(&mut chain_state, channel_state).map_err(Into::into)?;

	let old_description = &initiator_state.transfer_description;
	let filtered_route_states = routes::filter_acceptable_routes(
		payment_state.routes.clone(),
		payment_state.cancelled_channels.clone(),
		get_addresses_to_channels(&chain_state),
		old_description.token_network_address,
		our_address,
	);
	let transfer_description = TransferDescriptionWithSecretState {
		token_network_registry_address: old_description.token_network_registry_address,
		payment_identifier: old_description.payment_identifier,
		amount: old_description.amount,
		token_network_address: old_description.token_network_address,
		initiator: old_description.initiator,
		target: old_description.target,
		secret: state_change.secret,
		secrethash: state_change.secrethash,
		lock_timeout: old_description.lock_timeout,
	};
	let (sub_iteration, chain_state, events) =
		initiator::try_new_route(chain_state, filtered_route_states, transfer_description)
			.map_err(Into::into)?;

	if let Some(new_state) = sub_iteration {
		let secrethash = new_state.transfer.lock.secrethash;
		payment_state.initiator_transfers.insert(secrethash, new_state);
	}

	Ok(InitiatorManagerTransition { new_state: Some(payment_state), chain_state, events })
}

pub fn handle_lock_expired(
	mut chain_state: ChainState,
	payment_state: Option<InitiatorPaymentState>,
	state_change: ReceiveLockExpired,
) -> TransitionResult {
	let payment_state = match payment_state {
		Some(payment_state) => payment_state,
		None =>
			return Err(StateTransitionError {
				msg:
					"ReceiveLockExpired state change should be accompanied by a valid payment state"
						.to_owned(),
			}),
	};

	let initiator_state = match payment_state.initiator_transfers.get(&state_change.secrethash) {
		Some(initiator_state) => initiator_state.clone(),
		None =>
			return Ok(InitiatorManagerTransition {
				new_state: Some(payment_state),
				chain_state,
				events: vec![],
			}),
	};

	let channel_identifier = initiator_state.channel_identifier;
	let mut channel_state = match views::get_channel_by_canonical_identifier(
		&chain_state,
		CanonicalIdentifier {
			chain_identifier: chain_state.chain_id,
			token_network_address: initiator_state.transfer_description.token_network_address,
			channel_identifier,
		},
	) {
		Some(channel_state) => channel_state.clone(),
		None =>
			return Ok(InitiatorManagerTransition {
				new_state: Some(payment_state),
				chain_state,
				events: vec![],
			}),
	};

	let secrethash = initiator_state.transfer.lock.secrethash;
	let recipient_address = channel_state.partner_state.address;
	let recipient_metadata =
		views::get_address_metadata(recipient_address, payment_state.routes.clone());
	let mut sub_iteration = channel::handle_receive_lock_expired(
		&mut channel_state,
		state_change,
		chain_state.block_number,
		recipient_metadata,
	)?;

	let channel_state = match sub_iteration.new_state {
		Some(channel_state) => channel_state,
		None =>
			return Err(StateTransitionError {
				msg: "handle_receive_lock_expired should not delete the task".to_owned(),
			}),
	};

	if channel::views::get_lock(&channel_state.partner_state, secrethash).is_none() {
		let transfer = initiator_state.transfer;
		let unlock_failed = ErrorUnlockClaimFailed {
			identifier: transfer.payment_identifier,
			secrethash,
			reason: "Lock expired".to_owned(),
		};
		sub_iteration.events.push(unlock_failed.into());
	}
	utils::update_channel(&mut chain_state, channel_state).map_err(Into::into)?;

	Ok(InitiatorManagerTransition {
		new_state: Some(payment_state),
		chain_state,
		events: sub_iteration.events,
	})
}

pub fn handle_secret_request(
	chain_state: ChainState,
	payment_state: Option<InitiatorPaymentState>,
	state_change: ReceiveSecretRequest,
) -> TransitionResult {
	let payment_state = match payment_state {
		Some(payment_state) => payment_state,
		None => {
			return Err(StateTransitionError {
                msg: "ReceiveSecretRequest state change should be accompanied by a valid payment state".to_owned(),
            });
		},
	};

	let initiator_state = match payment_state.initiator_transfers.get(&state_change.secrethash) {
		Some(initiator_state) => initiator_state.clone(),
		None =>
			return Ok(InitiatorManagerTransition {
				new_state: Some(payment_state),
				chain_state,
				events: vec![],
			}),
	};

	if initiator_state.transfer_state == TransferState::Canceled {
		return Ok(InitiatorManagerTransition {
			new_state: Some(payment_state),
			chain_state,
			events: vec![],
		})
	}

	subdispatch_to_initiator_transfer(
		chain_state,
		payment_state,
		&initiator_state,
		state_change.into(),
	)
}

pub fn handle_secret_reveal(
	chain_state: ChainState,
	payment_state: Option<InitiatorPaymentState>,
	state_change: ReceiveSecretReveal,
) -> TransitionResult {
	let payment_state = match payment_state {
		Some(payment_state) => payment_state,
		None => {
			return Err(StateTransitionError {
                msg: "ReceiveSecretReveal state change should be accompanied by a valid payment state".to_owned(),
            });
		},
	};

	let initiator_state = match payment_state.initiator_transfers.get(&state_change.secrethash) {
		Some(initiator_state) => initiator_state.clone(),
		None =>
			return Ok(InitiatorManagerTransition {
				new_state: Some(payment_state),
				chain_state,
				events: vec![],
			}),
	};

	let mut sub_iteration = subdispatch_to_initiator_transfer(
		chain_state,
		payment_state,
		&initiator_state,
		state_change.clone().into(),
	)?;

	if let Some(ref mut new_state) = sub_iteration.new_state {
		if !transfer_exists(&new_state, state_change.secrethash) {
			cancel_other_transfers(new_state);
		}
	}

	Ok(sub_iteration)
}

pub fn handle_contract_secret_reveal(
	chain_state: ChainState,
	payment_state: Option<InitiatorPaymentState>,
	state_change: ContractReceiveSecretReveal,
) -> TransitionResult {
	let payment_state = match payment_state {
		Some(payment_state) => payment_state,
		None => {
			return Err(StateTransitionError {
                msg: "ContractReceiveSecretReveal state change should be accompanied by a valid payment state"
                    .to_owned(),
            });
		},
	};

	let initiator_state = match payment_state.initiator_transfers.get(&state_change.secrethash) {
		Some(initiator_state) => initiator_state.clone(),
		None =>
			return Ok(InitiatorManagerTransition {
				new_state: Some(payment_state),
				chain_state,
				events: vec![],
			}),
	};

	if initiator_state.transfer_state == TransferState::Canceled {
		return Ok(InitiatorManagerTransition {
			new_state: Some(payment_state),
			chain_state,
			events: vec![],
		})
	}

	let mut sub_iteration = subdispatch_to_initiator_transfer(
		chain_state,
		payment_state,
		&initiator_state,
		state_change.clone().into(),
	)?;

	if let Some(ref mut new_state) = sub_iteration.new_state {
		if !transfer_exists(&new_state, state_change.secrethash) {
			cancel_other_transfers(new_state);
		}
	}

	Ok(sub_iteration)
}

pub fn clear_if_finalized(transition: InitiatorManagerTransition) -> InitiatorManagerTransition {
	if let Some(ref new_state) = transition.new_state {
		if new_state.initiator_transfers.len() == 0 {
			return InitiatorManagerTransition {
				new_state: None,
				chain_state: transition.chain_state,
				events: transition.events,
			}
		}
	}
	transition
}

pub fn state_transition(
	chain_state: ChainState,
	manager_state: Option<InitiatorPaymentState>,
	state_change: StateChange,
) -> TransitionResult {
	let transition_result = match state_change {
		StateChange::Block(inner) => handle_block(chain_state, manager_state, inner),
		StateChange::ActionInitInitiator(inner) =>
			handle_init_initiator(chain_state, manager_state, inner),
		StateChange::ActionTransferReroute(inner) =>
			handle_action_transfer_reroute(chain_state, manager_state, inner),
		StateChange::ActionCancelPayment(inner) =>
			handle_action_cancel_payment(chain_state, manager_state, inner),
		StateChange::ReceiveTransferCancelRoute(inner) =>
			handle_transfer_cancel_route(chain_state, manager_state, inner),
		StateChange::ReceiveSecretRequest(inner) =>
			handle_secret_request(chain_state, manager_state, inner),
		StateChange::ReceiveSecretReveal(inner) =>
			handle_secret_reveal(chain_state, manager_state, inner),
		StateChange::ReceiveLockExpired(inner) =>
			handle_lock_expired(chain_state, manager_state, inner),
		StateChange::ContractReceiveSecretReveal(inner) =>
			handle_contract_secret_reveal(chain_state, manager_state, inner),
		_ =>
			return Ok(InitiatorManagerTransition {
				new_state: manager_state,
				chain_state,
				events: vec![],
			}),
	}?;

	Ok(clear_if_finalized(transition_result))
}
