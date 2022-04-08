use crate::{
    errors::StateTransitionError,
    state_machine::types::{
        ChainState,
        Event,
        StateChange,
        TargetTransferState,
    },
};

pub(super) type TransitionResult = std::result::Result<TargetTransition, StateTransitionError>;

pub struct TargetTransition {
    pub new_state: Option<TargetTransferState>,
    pub chain_state: ChainState,
    pub events: Vec<Event>,
}

fn sanity_check(transition: TargetTransition) -> TransitionResult {
    Ok(transition)
}

pub fn clear_if_finalized(transition: TargetTransition) -> TargetTransition {
    transition
}

pub fn state_transition(
    chain_state: ChainState,
    target_state: Option<TargetTransferState>,
    state_change: StateChange,
) -> TransitionResult {
    let transition_result = match state_change {
        _ => {
            return Ok(TargetTransition {
                new_state: target_state,
                chain_state,
                events: vec![],
            });
        }
    }?;

    let transition_result = sanity_check(transition_result)?;
    Ok(clear_if_finalized(transition_result))
}
