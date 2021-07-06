use crate::errors::StateTransitionError;
use crate::state_machine::state::TokenNetworkState;
use crate::state_machine::types::{
    ContractReceiveChannelOpened,
    Event,
    StateChange,
};

pub struct TokenNetworkTransition {
    pub new_state: TokenNetworkState,
    pub events: Vec<Event>,
}

fn handle_contract_receive_channel_opened(
    mut token_network: TokenNetworkState,
    state_change: ContractReceiveChannelOpened,
) -> Result<TokenNetworkTransition, StateTransitionError> {
    token_network.channelidentifiers_to_channels.insert(
        state_change.channel_state.canonical_identifier.channel_identifier,
        state_change.channel_state,
    );
    Ok(TokenNetworkTransition {
        new_state: token_network,
        events: vec![],
    })
}

pub fn state_transition(
    token_network: TokenNetworkState,
    state_change: StateChange,
) -> Result<TokenNetworkTransition, StateTransitionError> {
    let result: Result<TokenNetworkTransition, StateTransitionError> = match state_change {
        StateChange::ContractReceiveChannelOpened(state_change) => {
            handle_contract_receive_channel_opened(token_network, state_change)
        }
        _ => Err(StateTransitionError {
            msg: String::from("Could not transition token network"),
        }),
    };
    result
}
