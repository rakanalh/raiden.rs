use web3::types::{
    H256,
    U64,
};

use crate::{
    errors::StateTransitionError,
    state_machine::{
        state::ChannelState,
        types::{
            Event,
            StateChange,
        },
    },
};

type TransitionResult = std::result::Result<ChannelTransition, StateTransitionError>;

pub struct ChannelTransition {
    pub new_state: Option<ChannelState>,
    pub events: Vec<Event>,
}

pub fn state_transition(
    channel_state: ChannelState,
    state_change: StateChange,
    block_number: U64,
    block_hash: H256,
) -> TransitionResult {
    match state_change {
        StateChange::Block(inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveChannelClosed(ref inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveChannelSettled(ref inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveChannelDeposit(ref inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveChannelWithdraw(ref inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveChannelBatchUnlock(ref inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveUpdateTransfer(ref inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        _ => Err(StateTransitionError {
            msg: String::from("Could not transition channel"),
        }),
    }
}
