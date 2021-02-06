use crate::enums::{Event, StateChange};
use crate::errors::StateTransitionError;
use crate::transfer::{
    event,
    state::{ChainState, TokenNetworkRegistryState, TokenNetworkState},
    state_change, token_network, views,
};

pub struct ChainTransition {
    pub new_state: ChainState,
    pub events: Vec<Event>,
}

fn handle_action_init_chain(
    state_change: state_change::ActionInitChain,
) -> Result<ChainTransition, StateTransitionError> {
    Ok(ChainTransition {
        new_state: ChainState::new(
            state_change.chain_id,
            state_change.block_number,
            state_change.our_address,
        ),
        events: vec![],
    })
}

fn handle_new_block(
    mut chain_state: ChainState,
    state_change: state_change::Block,
) -> Result<ChainTransition, StateTransitionError> {
    chain_state.block_number = state_change.block_number;
    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![],
    })
}

fn handle_contract_receive_token_network_registry(
    mut chain_state: ChainState,
    state_change: state_change::ContractReceiveTokenNetworkRegistry,
) -> Result<ChainTransition, StateTransitionError> {
    chain_state.identifiers_to_tokennetworkregistries.insert(
        state_change.token_network_registry.address,
        state_change.token_network_registry,
    );
    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![],
    })
}

fn handle_contract_receive_token_network_created(
    mut chain_state: ChainState,
    state_change: state_change::ContractReceiveTokenNetworkCreated,
) -> Result<ChainTransition, StateTransitionError> {
    let token_network_registries = &mut chain_state.identifiers_to_tokennetworkregistries;
    let mut token_network_registry = token_network_registries.get_mut(&state_change.token_network_registry_address);

    if token_network_registry.is_none() {
        return Err(StateTransitionError {
            msg: format!(
                "Token network registry {} was not found",
                state_change.token_network_registry_address
            ),
        });
    }

    let token_network_registry: &mut TokenNetworkRegistryState = token_network_registry.as_mut().unwrap();
    token_network_registry
        .tokennetworkaddresses_to_tokennetworks
        .insert(state_change.token_network.address, state_change.token_network.clone());
    token_network_registry.tokenaddresses_to_tokennetworkaddresses.insert(
        state_change.token_network.token_address.clone(),
        state_change.token_network.address.clone(),
    );
    drop(token_network_registries);

    let token_network_created: event::TokenNetworkCreated = state_change.into();

    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![Event::TokenNetworkCreated(token_network_created)],
    })
}

fn handle_token_network_state_change(
    mut chain_state: ChainState,
    state_change: state_change::ContractReceiveChannelOpened,
) -> Result<ChainTransition, StateTransitionError> {
    let token_network_state = views::get_token_network(
        &chain_state,
        &state_change.channel_state.canonical_identifier.token_network_address,
    );
    if token_network_state.is_none() {
        return Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        });
    }
    let token_network_state = token_network_state.unwrap().clone();
    let result = token_network::state_transition(
        token_network_state,
        StateChange::ContractReceiveChannelOpened(state_change),
    );

    if let Ok(transition) = result {
        let new_state: TokenNetworkState = transition.new_state;
        let registry_address =
            views::get_token_network_registry_by_token_network_address(&chain_state, new_state.address)
                .unwrap()
                .address;
        let registry = chain_state
            .identifiers_to_tokennetworkregistries
            .get_mut(&registry_address)
            .unwrap();
        registry
            .tokennetworkaddresses_to_tokennetworks
            .insert(new_state.address, new_state);
    }

    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![],
    })
}

pub fn state_transition(
    chain_state: Option<ChainState>,
    state_change: StateChange,
) -> Result<ChainTransition, StateTransitionError> {
    let result: Result<ChainTransition, StateTransitionError> = match state_change {
        StateChange::ActionInitChain(state_change) => handle_action_init_chain(state_change),
        StateChange::Block(state_change) => handle_new_block(chain_state.unwrap(), state_change),
        StateChange::ContractReceiveTokenNetworkRegistry(state_change) => {
            handle_contract_receive_token_network_registry(chain_state.unwrap(), state_change)
        }
        StateChange::ContractReceiveTokenNetworkCreated(state_change) => {
            handle_contract_receive_token_network_created(chain_state.unwrap(), state_change)
        }
        StateChange::ContractReceiveChannelOpened(state_change) => {
            handle_token_network_state_change(chain_state.unwrap(), state_change)
        }
    };
    result
}
