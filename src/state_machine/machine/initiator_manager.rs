use std::collections::HashMap;

use super::initiator;
use crate::{
    errors::StateTransitionError,
    primitives::CanonicalIdentifier,
    state_machine::{
        types::{
            ActionCancelPayment,
            ActionInitInitiator,
            ActionTransferReroute,
            Block,
            ChainState,
            ContractReceiveSecretReveal,
            Event,
            InitiatorPaymentState,
            InitiatorTransferState,
            ReceiveLockExpired,
            ReceiveSecretRequest,
            ReceiveSecretReveal,
            ReceiveTransferCancelRoute,
            StateChange,
        },
        views,
    },
};

pub(super) type TransitionResult = std::result::Result<InitiatorTransition, StateTransitionError>;

pub struct InitiatorTransition {
    pub new_state: Option<InitiatorPaymentState>,
    pub chain_state: ChainState,
    pub events: Vec<Event>,
}

fn subdispatch_to_initiator_transfer(
    chain_state: ChainState,
    mut payment_state: InitiatorPaymentState,
    initiator_state: InitiatorTransferState,
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
        None => {
            return Ok(InitiatorTransition {
                new_state: Some(payment_state),
                chain_state,
                events: vec![],
            });
        }
    };

    let sub_iteration = initiator::state_transition(
        initiator_state.clone(),
        state_change,
        channel_state.clone(),
        chain_state.pseudo_random_number_generator.clone(),
        chain_state.block_number,
    )?;

    match sub_iteration.new_state {
        Some(transfer_state) => {
            payment_state
                .initiator_transfers
                .insert(initiator_state.transfer.lock.secrethash, transfer_state);
        }
        None => {
            payment_state
                .initiator_transfers
                .remove(&initiator_state.transfer.lock.secrethash);
        }
    }

    Ok(InitiatorTransition {
        new_state: Some(payment_state),
        chain_state,
        events: sub_iteration.events,
    })
}

fn subdispatch_to_all_initiator_transfer(
    payment_state: InitiatorPaymentState,
    chain_state: ChainState,
    state_change: StateChange,
) -> TransitionResult {
    let mut events = vec![];
    for (secrethash, initiator_state) in &payment_state.initiator_transfers {
        let sub_iteration = subdispatch_to_initiator_transfer(
            chain_state.clone(),
            payment_state.clone(),
            initiator_state.clone(),
            state_change.clone(),
        )?;
        events.extend(sub_iteration.events);
    }

    Ok(InitiatorTransition {
        new_state: Some(payment_state),
        chain_state,
        events,
    })
}

pub fn handle_block(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: Block,
) -> TransitionResult {
    Ok(InitiatorTransition {
        chain_state,
        new_state: payment_state,
        events: vec![],
    })
}

pub fn handle_init_initiator(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: ActionInitInitiator,
) -> TransitionResult {
    let mut payment_state = payment_state.clone();
    let mut events = vec![];
    if payment_state.is_none() {
        let (new_state, chain_state, iteration_events) =
            initiator::try_new_route(chain_state.clone(), state_change.routes.clone(), state_change.transfer)?;

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

    Ok(InitiatorTransition {
        new_state: payment_state,
        chain_state,
        events,
    })
}

pub fn handle_action_transfer_reroute(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: ActionTransferReroute,
) -> TransitionResult {
    Ok(InitiatorTransition {
        new_state: payment_state,
        chain_state,
        events: vec![],
    })
}

pub fn handle_action_cancel_payment(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: ActionCancelPayment,
) -> TransitionResult {
    Ok(InitiatorTransition {
        new_state: payment_state,
        chain_state,
        events: vec![],
    })
}

pub fn handle_transfer_cancel_route(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: ReceiveTransferCancelRoute,
) -> TransitionResult {
    Ok(InitiatorTransition {
        new_state: payment_state,
        chain_state,
        events: vec![],
    })
}

pub fn handle_secret_request(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: ReceiveSecretRequest,
) -> TransitionResult {
    Ok(InitiatorTransition {
        new_state: payment_state,
        chain_state,
        events: vec![],
    })
}

pub fn handle_secret_reveal(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: ReceiveSecretReveal,
) -> TransitionResult {
    Ok(InitiatorTransition {
        new_state: payment_state,
        chain_state,
        events: vec![],
    })
}

pub fn handle_lock_expired(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: ReceiveLockExpired,
) -> TransitionResult {
    Ok(InitiatorTransition {
        new_state: payment_state,
        chain_state,
        events: vec![],
    })
}

pub fn handle_contract_secret_reveal(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: ContractReceiveSecretReveal,
) -> TransitionResult {
    Ok(InitiatorTransition {
        new_state: payment_state,
        chain_state,
        events: vec![],
    })
}

pub fn clear_if_finalized(transition: InitiatorTransition) -> InitiatorTransition {
    if let Some(ref new_state) = transition.new_state {
        if new_state.initiator_transfers.len() == 0 {
            return InitiatorTransition {
                new_state: None,
                chain_state: transition.chain_state,
                events: transition.events,
            };
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
        StateChange::ActionInitInitiator(inner) => handle_init_initiator(chain_state, manager_state, inner),
        StateChange::ActionTransferReroute(inner) => handle_action_transfer_reroute(chain_state, manager_state, inner),
        StateChange::ActionCancelPayment(inner) => handle_action_cancel_payment(chain_state, manager_state, inner),
        StateChange::ReceiveTransferCancelRoute(inner) => {
            handle_transfer_cancel_route(chain_state, manager_state, inner)
        }
        StateChange::ReceiveSecretRequest(inner) => handle_secret_request(chain_state, manager_state, inner),
        StateChange::ReceiveSecretReveal(inner) => handle_secret_reveal(chain_state, manager_state, inner),
        StateChange::ReceiveLockExpired(inner) => handle_lock_expired(chain_state, manager_state, inner),
        StateChange::ContractReceiveSecretReveal(inner) => {
            handle_contract_secret_reveal(chain_state, manager_state, inner)
        }
        _ => {
            return Ok(InitiatorTransition {
                new_state: manager_state,
                chain_state,
                events: vec![],
            })
        }
    }?;

    Ok(clear_if_finalized(transition_result))
}
