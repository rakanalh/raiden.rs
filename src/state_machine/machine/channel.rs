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

pub struct ChannelTransition {
    pub new_state: ChannelState,
    pub events: Vec<Event>,
}

pub fn state_transition(
    channel_state: ChannelState,
    _state_change: StateChange,
    _block_number: U64,
    _block_hash: H256,
) -> Result<ChannelTransition, StateTransitionError> {
    Ok(ChannelTransition {
        new_state: channel_state,
        events: vec![],
    })
}
