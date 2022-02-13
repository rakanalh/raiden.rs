use std::collections::HashMap;

use super::initiator;
use crate::{
    errors::StateTransitionError,
    state_machine::types::{
        ActionInitInitiator,
        ChainState,
        Event,
        InitiatorPaymentState,
        StateChange,
    },
};

pub(super) type TransitionResult = std::result::Result<InitiatorTransition, StateTransitionError>;

pub struct InitiatorTransition {
    pub new_state: Option<InitiatorPaymentState>,
    pub events: Vec<Event>,
}

pub fn initiate(
    chain_state: ChainState,
    payment_state: Option<InitiatorPaymentState>,
    state_change: ActionInitInitiator,
) -> TransitionResult {
    let mut payment_state = payment_state.clone();
    let mut events = vec![];
    if payment_state.is_none() {
        let sub_iteration = initiator::try_new_route(chain_state, state_change.routes.clone(), state_change.transfer)?;

        events = sub_iteration.events;
        if let Some(new_state) = sub_iteration.new_state {
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
        events,
    })
}

pub fn state_transition(mut _manager_state: InitiatorPaymentState, state_change: StateChange) -> TransitionResult {
    match state_change {
        StateChange::Block(_) => todo!(),
        StateChange::ActionInitChain(_) => todo!(),
        StateChange::ActionInitInitiator(_) => todo!(),
        StateChange::ActionChannelSetRevealTimeout(_) => todo!(),
        StateChange::ActionChannelWithdraw(_) => todo!(),
        StateChange::ContractReceiveTokenNetworkRegistry(_) => todo!(),
        StateChange::ContractReceiveTokenNetworkCreated(_) => todo!(),
        StateChange::ContractReceiveChannelOpened(_) => todo!(),
        StateChange::ContractReceiveChannelClosed(_) => todo!(),
        StateChange::ContractReceiveChannelSettled(_) => todo!(),
        StateChange::ContractReceiveChannelDeposit(_) => todo!(),
        StateChange::ContractReceiveChannelWithdraw(_) => todo!(),
        StateChange::ContractReceiveChannelBatchUnlock(_) => todo!(),
        StateChange::ContractReceiveSecretReveal(_) => todo!(),
        StateChange::ContractReceiveRouteNew(_) => todo!(),
        StateChange::ContractReceiveUpdateTransfer(_) => todo!(),
    }
}
