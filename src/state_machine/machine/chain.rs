use web3::types::{Address, H256, U64};

use crate::state_machine::{
    machine::{
        channel,
        token_network,
    },
    types::{
        ActionInitChain,
        Block,
        ContractReceiveTokenNetworkCreated,
        ContractReceiveTokenNetworkRegistry,
        Event,
        StateChange,
        TokenNetworkCreated,
    },
    views,
};
use crate::{
    errors::StateTransitionError,
    state_machine::state::{
        ChainState,
        TokenNetworkState,
    },
};

pub struct ChainTransition {
    pub new_state: ChainState,
    pub events: Vec<Event>,
}

fn subdispatch_to_all_channels(
    mut chain_state: ChainState,
    state_change: StateChange,
    block_number: U64,
    block_hash: H256,
) -> Result<ChainTransition, StateTransitionError> {
    let mut events = vec![];

    for (_, token_network_registry) in chain_state.identifiers_to_tokennetworkregistries.iter_mut() {
        for (_, token_network) in token_network_registry.tokennetworkaddresses_to_tokennetworks.iter_mut() {
            for (_, channel_state) in token_network.channelidentifiers_to_channels.iter_mut() {
                let result =
                    channel::state_transition(channel_state.clone(), state_change.clone(), block_number, block_hash)?;

                *channel_state = result.new_state;
                events.extend(result.events);
            }
        }
    }

    Ok(ChainTransition {
        new_state: chain_state,
        events,
    })
}

fn subdispatch_to_payment_task(
    chain_state: ChainState,
    _state_change: StateChange,
    _secrethash: H256,
) -> Result<ChainTransition, StateTransitionError> {
    // @TODO: Implement this
    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![],
    })
}

fn subdispatch_to_all_lockedtransfers(
    mut chain_state: ChainState,
    state_change: StateChange,
) -> Result<ChainTransition, StateTransitionError> {
    let mut events = vec![];

    let payment_mapping = chain_state.payment_mapping.clone();
    for secrethash in payment_mapping.secrethashes_to_task.keys() {
        let result = subdispatch_to_payment_task(chain_state.clone(), state_change.clone(), *secrethash)?;
        chain_state = result.new_state;
        events.extend(result.events);
    }

    Ok(ChainTransition {
        new_state: chain_state,
        events,
    })
}

fn handle_action_init_chain(state_change: ActionInitChain) -> Result<ChainTransition, StateTransitionError> {
    Ok(ChainTransition {
        new_state: ChainState::new(
            state_change.chain_id,
            state_change.block_number,
            state_change.block_hash,
            state_change.our_address,
        ),
        events: vec![],
    })
}

fn handle_new_block(mut chain_state: ChainState, state_change: Block) -> Result<ChainTransition, StateTransitionError> {
    chain_state.block_number = state_change.block_number;
    chain_state.block_hash = state_change.block_hash;

    let channels_result = subdispatch_to_all_channels(
        chain_state.clone(),
        StateChange::Block(state_change.clone()),
        chain_state.block_number,
        chain_state.block_hash,
    )?;

    let mut events = channels_result.events;

    let chain_state = channels_result.new_state;

    let transfers_result = subdispatch_to_all_lockedtransfers(chain_state, StateChange::Block(state_change))?;
    events.extend(transfers_result.events);

    let chain_state = transfers_result.new_state;

    Ok(ChainTransition {
        new_state: chain_state,
        events,
    })
}

fn handle_contract_receive_token_network_registry(
    mut chain_state: ChainState,
    state_change: ContractReceiveTokenNetworkRegistry,
) -> Result<ChainTransition, StateTransitionError> {
    chain_state
        .identifiers_to_tokennetworkregistries
        .entry(state_change.token_network_registry.address)
        .or_insert(state_change.token_network_registry);

    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![],
    })
}

fn handle_contract_receive_token_network_created(
    mut chain_state: ChainState,
    state_change: ContractReceiveTokenNetworkCreated,
) -> Result<ChainTransition, StateTransitionError> {
    let token_network_registries = &mut chain_state.identifiers_to_tokennetworkregistries;
    let token_network_registry = match token_network_registries.get_mut(&state_change.token_network_registry_address) {
        Some(token_network_registry) => token_network_registry,
        None => {
            return Err(StateTransitionError {
                msg: format!(
                    "Token network registry {} was not found",
                    state_change.token_network_registry_address
                ),
            });
        }
    };

    token_network_registry
        .tokennetworkaddresses_to_tokennetworks
        .insert(state_change.token_network.address, state_change.token_network.clone());
    token_network_registry.tokenaddresses_to_tokennetworkaddresses.insert(
        state_change.token_network.token_address.clone(),
        state_change.token_network.address.clone(),
    );

    let token_network_created: TokenNetworkCreated = state_change.into();

    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![Event::TokenNetworkCreated(token_network_created)],
    })
}

fn handle_token_network_state_change(
    mut chain_state: ChainState,
    token_network_address: Address,
    state_change: StateChange,
) -> Result<ChainTransition, StateTransitionError> {
    let token_network_state = match views::get_token_network(
        &chain_state,
        &token_network_address,
    ) {
        Some(token_network_state) => token_network_state,
        None => {
            return Err(StateTransitionError {
                msg: format!(
                    "Token network {} was not found",
                    token_network_address,
                ),
            });
        }
    };

    let transition = token_network::state_transition(
        token_network_state.clone(),
        state_change,
    )?;

    let new_state: TokenNetworkState = transition.new_state;
    let registry_address = views::get_token_network_registry_by_token_network_address(&chain_state, new_state.address)
        .unwrap()
        .address;
    let registry = chain_state
        .identifiers_to_tokennetworkregistries
        .get_mut(&registry_address)
        .unwrap();
    registry
        .tokennetworkaddresses_to_tokennetworks
        .insert(new_state.address, new_state);

    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![],
    })
}

pub fn state_transition(
    chain_state: ChainState,
    state_change: StateChange,
) -> Result<ChainTransition, StateTransitionError> {
    let result: Result<ChainTransition, StateTransitionError> = match state_change {
        StateChange::ActionInitChain(inner) => handle_action_init_chain(inner),
        StateChange::Block(inner) => handle_new_block(chain_state, inner),
        StateChange::ContractReceiveTokenNetworkRegistry(inner) => {
            handle_contract_receive_token_network_registry(chain_state, inner)
        }
        StateChange::ContractReceiveTokenNetworkCreated(inner) => {
            handle_contract_receive_token_network_created(chain_state, inner)
        }
        StateChange::ContractReceiveChannelOpened(ref inner) => {
            let token_network_address = inner.channel_state.canonical_identifier.token_network_address;
            handle_token_network_state_change(chain_state, token_network_address, state_change)
        }
        StateChange::ContractReceiveChannelClosed(_state_change) => Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        }),
        StateChange::ContractReceiveChannelSettled(_state_change) => Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        }),
        StateChange::ContractReceiveChannelDeposit(_) => Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        }),
        StateChange::ContractReceiveChannelWithdraw(_) => Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        }),
        StateChange::ContractReceiveChannelBatchUnlock(_) => Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        }),
        StateChange::ContractReceiveSecretReveal(_) => Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        }),
        StateChange::ContractReceiveRouteNew(_) => Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        }),
        StateChange::ContractReceiveUpdateTransfer(_) => Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        }),
    };
    result
}
