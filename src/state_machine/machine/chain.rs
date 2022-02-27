use web3::types::H256;

use crate::{
    errors::StateTransitionError,
    primitives::{
        CanonicalIdentifier,
        TokenNetworkAddress,
        U64,
    },
    state_machine::types::{
        ActionInitInitiator,
        ChainState,
        InitiatorTask,
        TokenNetworkState,
        TransferRole,
        TransferTask,
    },
};
use crate::{
    primitives::QueueIdentifier,
    state_machine::{
        machine::{
            channel,
            token_network,
        },
        types::{
            ActionInitChain,
            Block,
            ContractReceiveChannelClosed,
            ContractReceiveTokenNetworkCreated,
            ContractReceiveTokenNetworkRegistry,
            Event,
            StateChange,
        },
        views,
    },
};

use super::initiator_manager;

type TransitionResult = std::result::Result<ChainTransition, StateTransitionError>;

pub struct ChainTransition {
    pub new_state: ChainState,
    pub events: Vec<Event>,
}

fn subdispatch_by_canonical_id(
    chain_state: &mut ChainState,
    state_change: StateChange,
    canonical_identifier: CanonicalIdentifier,
) -> TransitionResult {
    let token_network_registries = &mut chain_state.identifiers_to_tokennetworkregistries;
    let token_network = match token_network_registries
        .values_mut()
        .map(|tnr| tnr.tokennetworkaddresses_to_tokennetworks.values_mut())
        .flatten()
        .find(|tn| tn.address == canonical_identifier.token_network_address)
    {
        Some(tn) => tn,
        None => {
            return Ok(ChainTransition {
                new_state: chain_state.clone(),
                events: vec![],
            })
        }
    };

    let transition = token_network::state_transition(
        token_network.clone(),
        state_change,
        chain_state.block_number,
        chain_state.block_hash,
        chain_state.pseudo_random_number_generator.clone(),
    )?;

    *token_network = transition.new_state;
    let events = transition.events;

    Ok(ChainTransition {
        new_state: chain_state.clone(),
        events,
    })
}

fn subdispatch_to_all_channels(
    mut chain_state: ChainState,
    state_change: StateChange,
    block_number: U64,
    block_hash: H256,
) -> TransitionResult {
    let mut events = vec![];

    for (_, token_network_registry) in chain_state.identifiers_to_tokennetworkregistries.iter_mut() {
        for (_, token_network) in token_network_registry.tokennetworkaddresses_to_tokennetworks.iter_mut() {
            for (_, channel_state) in token_network.channelidentifiers_to_channels.iter_mut() {
                let result = channel::state_transition(
                    channel_state.clone(),
                    state_change.clone(),
                    block_number,
                    block_hash,
                    chain_state.pseudo_random_number_generator.clone(),
                )?;

                if let Some(new_state) = result.new_state {
                    *channel_state = new_state;
                }
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
) -> TransitionResult {
    // @TODO: Implement this
    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![],
    })
}

fn subdispatch_to_all_lockedtransfers(mut chain_state: ChainState, state_change: StateChange) -> TransitionResult {
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

fn subdispatch_initiator_task(mut chain_state: ChainState, state_change: ActionInitInitiator) -> TransitionResult {
    let token_network_state =
        match views::get_token_network_by_address(&chain_state, state_change.transfer.token_network_address) {
            Some(tn) => tn.clone(),
            None => {
                return Ok(ChainTransition {
                    new_state: chain_state,
                    events: vec![],
                });
            }
        };

    let manager_state = match chain_state
        .payment_mapping
        .secrethashes_to_task
        .get(&state_change.transfer.secrethash)
    {
        Some(sub_task) => {
            let initiator = match sub_task {
                TransferTask::Initiator(initiator)
                    if token_network_state.address == initiator.token_network_address =>
                {
                    initiator
                }
                _ => {
                    return Ok(ChainTransition {
                        new_state: chain_state,
                        events: vec![],
                    });
                }
            };
            Some(initiator.manager_state.clone())
        }
        None => None,
    };

    if manager_state.is_some() {
        return Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        });
    }

    let initiator_state = initiator_manager::state_transition(
        chain_state.clone(),
        manager_state,
        StateChange::ActionInitInitiator(state_change.clone()),
    )?;

    match initiator_state.new_state {
        Some(initiator_state) => {
            chain_state.payment_mapping.secrethashes_to_task.insert(
                state_change.transfer.secrethash,
                TransferTask::Initiator(InitiatorTask {
                    role: TransferRole::Initiator,
                    token_network_address: token_network_state.address.clone(),
                    manager_state: initiator_state,
                }),
            );
        }
        None => {
            chain_state
                .payment_mapping
                .secrethashes_to_task
                .remove(&state_change.transfer.secrethash);
        }
    }

    Ok(ChainTransition {
        new_state: chain_state,
        events: initiator_state.events,
    })
}

fn handle_action_init_chain(state_change: ActionInitChain) -> TransitionResult {
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

fn handle_action_init_intiator(chain_state: ChainState, state_change: ActionInitInitiator) -> TransitionResult {
    subdispatch_initiator_task(chain_state, state_change)
}

fn handle_new_block(mut chain_state: ChainState, state_change: Block) -> TransitionResult {
    chain_state.block_number = state_change.block_number;
    chain_state.block_hash = state_change.block_hash;

    let channels_result = subdispatch_to_all_channels(
        chain_state.clone(),
        StateChange::Block(state_change.clone()),
        chain_state.block_number,
        chain_state.block_hash,
    )?;

    let mut events = channels_result.events;

    chain_state = channels_result.new_state;

    let transfers_result = subdispatch_to_all_lockedtransfers(chain_state, StateChange::Block(state_change))?;
    events.extend(transfers_result.events);

    chain_state = transfers_result.new_state;

    Ok(ChainTransition {
        new_state: chain_state,
        events,
    })
}

fn handle_contract_receive_token_network_registry(
    mut chain_state: ChainState,
    state_change: ContractReceiveTokenNetworkRegistry,
) -> TransitionResult {
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
) -> TransitionResult {
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

    Ok(ChainTransition {
        new_state: chain_state,
        events: vec![],
    })
}

fn handle_token_network_state_change(
    mut chain_state: ChainState,
    token_network_address: TokenNetworkAddress,
    state_change: StateChange,
    block_number: U64,
    block_hash: H256,
) -> TransitionResult {
    let token_network_state = match views::get_token_network(&chain_state, &token_network_address) {
        Some(token_network_state) => token_network_state,
        None => {
            return Err(StateTransitionError {
                msg: format!("Token network {} was not found", token_network_address,),
            });
        }
    };

    let transition = token_network::state_transition(
        token_network_state.clone(),
        state_change,
        block_number,
        block_hash,
        chain_state.pseudo_random_number_generator.clone(),
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
        events: transition.events,
    })
}

fn handle_contract_receive_channel_closed(
    mut chain_state: ChainState,
    state_change: ContractReceiveChannelClosed,
    block_number: U64,
    block_hash: H256,
) -> TransitionResult {
    let token_network_address = state_change.canonical_identifier.token_network_address;
    if let Some(channel_state) =
        views::get_channel_by_canonical_identifier(&chain_state, state_change.canonical_identifier.clone())
    {
        let queue_identifier = QueueIdentifier {
            recipient: channel_state.partner_state.address,
            canonical_identifier: state_change.canonical_identifier.clone(),
        };
        chain_state.queueids_to_queues.remove(&queue_identifier);
    }
    handle_token_network_state_change(
        chain_state,
        token_network_address,
        StateChange::ContractReceiveChannelClosed(state_change),
        block_number,
        block_hash,
    )
}

pub fn state_transition(mut chain_state: ChainState, state_change: StateChange) -> TransitionResult {
    match state_change {
        StateChange::ActionInitChain(inner) => handle_action_init_chain(inner),
        StateChange::ActionInitInitiator(inner) => handle_action_init_intiator(chain_state, inner),
        StateChange::ActionChannelWithdraw(ref inner) => subdispatch_by_canonical_id(
            &mut chain_state,
            state_change.clone(),
            inner.canonical_identifier.clone(),
        ),
        StateChange::ActionChannelSetRevealTimeout(ref inner) => subdispatch_by_canonical_id(
            &mut chain_state,
            state_change.clone(),
            inner.canonical_identifier.clone(),
        ),
        StateChange::Block(inner) => handle_new_block(chain_state, inner),
        StateChange::ContractReceiveTokenNetworkRegistry(inner) => {
            handle_contract_receive_token_network_registry(chain_state, inner)
        }
        StateChange::ContractReceiveTokenNetworkCreated(inner) => {
            handle_contract_receive_token_network_created(chain_state, inner)
        }
        StateChange::ContractReceiveChannelOpened(ref inner) => {
            let token_network_address = inner.channel_state.canonical_identifier.token_network_address;
            handle_token_network_state_change(
                chain_state.clone(),
                token_network_address,
                state_change,
                chain_state.block_number,
                chain_state.block_hash,
            )
        }
        StateChange::ContractReceiveChannelClosed(inner) => handle_contract_receive_channel_closed(
            chain_state.clone(),
            inner,
            chain_state.block_number,
            chain_state.block_hash,
        ),
        StateChange::ContractReceiveChannelSettled(ref inner) => {
            let token_network_address = inner.canonical_identifier.token_network_address;
            handle_token_network_state_change(
                chain_state.clone(),
                token_network_address,
                state_change,
                chain_state.block_number,
                chain_state.block_hash,
            )
        }
        StateChange::ContractReceiveChannelDeposit(ref inner) => {
            let token_network_address = inner.canonical_identifier.token_network_address;
            handle_token_network_state_change(
                chain_state.clone(),
                token_network_address,
                state_change,
                chain_state.block_number,
                chain_state.block_hash,
            )
        }
        StateChange::ContractReceiveChannelWithdraw(ref inner) => {
            let token_network_address = inner.canonical_identifier.token_network_address;
            handle_token_network_state_change(
                chain_state.clone(),
                token_network_address,
                state_change,
                chain_state.block_number,
                chain_state.block_hash,
            )
        }
        StateChange::ContractReceiveChannelBatchUnlock(ref inner) => {
            let token_network_address = inner.canonical_identifier.token_network_address;
            handle_token_network_state_change(
                chain_state.clone(),
                token_network_address,
                state_change,
                chain_state.block_number,
                chain_state.block_hash,
            )
        }
        StateChange::ContractReceiveUpdateTransfer(ref inner) => {
            let token_network_address = inner.canonical_identifier.token_network_address;
            handle_token_network_state_change(
                chain_state.clone(),
                token_network_address,
                state_change,
                chain_state.block_number,
                chain_state.block_hash,
            )
        }
        StateChange::ContractReceiveSecretReveal(ref inner) => {
            subdispatch_to_payment_task(chain_state, state_change.clone(), inner.secrethash)
        }
        StateChange::ContractReceiveRouteNew(_) => Ok(ChainTransition {
            new_state: chain_state,
            events: vec![],
        }),
        StateChange::ActionTransferReroute(_) => todo!(),
        StateChange::ActionCancelPayment(_) => todo!(),
        StateChange::ReceiveTransferCancelRoute(_) => todo!(),
        StateChange::ReceiveSecretReveal(_) => todo!(),
        StateChange::ReceiveSecretRequest(_) => todo!(),
        StateChange::ReceiveLockExpired(_) => todo!(),
    }
}
