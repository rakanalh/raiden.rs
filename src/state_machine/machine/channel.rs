use web3::types::{
    H256,
    U64,
};

use crate::{
    constants::CHANNEL_STATES_PRIOR_TO_CLOSE,
    errors::StateTransitionError,
    primitives::{
        Random,
        TransactionExecutionStatus,
        TransactionResult,
    },
    state_machine::types::{
        Block,
        ChannelState,
        ChannelStatus,
        ContractReceiveChannelClosed,
        ContractSendChannelSettle,
        ContractSendChannelUpdateTransfer,
        ContractSendEvent,
        Event,
        ExpiredWithdrawState,
        SendMessageEventInner,
        SendWithdrawExpired,
        StateChange,
    },
};

type TransitionResult = std::result::Result<ChannelTransition, StateTransitionError>;

pub struct ChannelTransition {
    pub new_state: Option<ChannelState>,
    pub events: Vec<Event>,
}

fn send_expired_withdraws(
    mut channel_state: ChannelState,
    block_number: U64,
    mut pseudo_random_number_generator: Random,
) -> Vec<Event> {
    let mut events = vec![];

    let withdraws_pending = channel_state.our_state.withdraws_pending.clone();
    for withdraw_state in withdraws_pending.values() {
        if !withdraw_state.has_expired(block_number) {
            continue;
        }

        let nonce = channel_state.our_state.next_nonce();
        channel_state.our_state.nonce = nonce;

        channel_state.our_state.withdraws_expired.push(ExpiredWithdrawState {
            total_withdraw: withdraw_state.total_withdraw,
            expiration: withdraw_state.expiration,
            nonce: withdraw_state.nonce,
            recipient_metadata: withdraw_state.recipient_metadata.clone(),
        });

        channel_state
            .our_state
            .withdraws_pending
            .remove(&withdraw_state.total_withdraw);

        events.push(Event::SendWithdrawExpired(SendWithdrawExpired {
            inner: SendMessageEventInner {
                recipient: channel_state.partner_state.address,
                recipient_metadata: withdraw_state.recipient_metadata.clone(),
                canonincal_identifier: channel_state.canonical_identifier.clone(),
                message_identifier: pseudo_random_number_generator.next(),
            },
            participant: channel_state.our_state.address,
            nonce: channel_state.our_state.nonce,
            expiration: withdraw_state.expiration,
        }));
    }

    events
}

fn handle_block(
    mut channel_state: ChannelState,
    state_change: Block,
    block_number: U64,
    pseudo_random_number_generator: Random,
) -> TransitionResult {
    let mut events = vec![];

    if channel_state.status() == ChannelStatus::Opened {
        let expired_withdraws =
            send_expired_withdraws(channel_state.clone(), block_number, pseudo_random_number_generator);
        events.extend(expired_withdraws)
    }

    if channel_state.status() == ChannelStatus::Closed {
        let close_transaction = match channel_state.close_transaction {
            Some(ref transaction) => transaction,
            None => {
                return Err(StateTransitionError {
                    msg: "Channel is Closed but close_transaction is not set".to_string(),
                });
            }
        };
        let closed_block_number = match close_transaction.finished_block_number {
            Some(number) => number,
            None => {
                return Err(StateTransitionError {
                    msg: "Channel is Closed but close_transaction block number is missing".to_string(),
                });
            }
        };

        let settlement_end = channel_state.settle_timeout + closed_block_number;
        if state_change.block_number > settlement_end {
            channel_state.settle_transaction = Some(TransactionExecutionStatus {
                started_block_number: Some(state_change.block_number),
                finished_block_number: None,
                result: None,
            });

            events.push(Event::ContractSendChannelSettle(ContractSendChannelSettle {
                inner: ContractSendEvent {
                    triggered_by_blockhash: state_change.block_hash,
                },
                canonical_identifier: channel_state.canonical_identifier.clone(),
            }));
        }
    }

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn set_closed(mut channel_state: ChannelState, block_number: U64) -> ChannelState {
    if channel_state.close_transaction.is_none() {
        channel_state.close_transaction = Some(TransactionExecutionStatus {
            started_block_number: None,
            finished_block_number: Some(block_number),
            result: Some(TransactionResult::Success),
        });
    } else if let Some(ref mut close_transaction) = channel_state.close_transaction {
        if close_transaction.finished_block_number.is_none() {
            close_transaction.finished_block_number = Some(block_number);
            close_transaction.result = Some(TransactionResult::Success);
        }
    }

    channel_state
}

fn handle_channel_closed(channel_state: ChannelState, state_change: ContractReceiveChannelClosed) -> TransitionResult {
    let mut events = vec![];

    let just_closed = state_change.canonical_identifier.chain_identifier
        == channel_state.canonical_identifier.chain_identifier
        && CHANNEL_STATES_PRIOR_TO_CLOSE
            .to_vec()
            .iter()
            .position(|status| status == &channel_state.status())
            .is_some();

    if just_closed {
        let mut channel_state = set_closed(channel_state.clone(), state_change.block_number);

        let balance_proof = match channel_state.partner_state.balance_proof {
            Some(bp) => bp,
            None => {
                return Ok(ChannelTransition {
                    new_state: Some(channel_state),
                    events: vec![],
                })
            }
        };
        let call_update = state_change.transaction_from != channel_state.our_state.address
            && channel_state.update_transaction.is_none();
        if call_update {
            let expiration = state_change.block_number + channel_state.settle_timeout;
            let update = Event::ContractSendChannelUpdateTransfer(ContractSendChannelUpdateTransfer {
                inner: ContractSendEvent {
                    triggered_by_blockhash: state_change.block_hash,
                },
                balance_proof,
                expiration,
            });
            channel_state.update_transaction = Some(TransactionExecutionStatus {
                started_block_number: Some(state_change.block_number),
                finished_block_number: None,
                result: None,
            });
            events.push(update);
        }
    }

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

pub fn state_transition(
    channel_state: ChannelState,
    state_change: StateChange,
    block_number: U64,
    block_hash: H256,
    pseudo_random_number_generator: Random,
) -> TransitionResult {
    match state_change {
        StateChange::Block(inner) => handle_block(channel_state, inner, block_number, pseudo_random_number_generator),
        StateChange::ContractReceiveChannelClosed(inner) => handle_channel_closed(channel_state, inner),
        StateChange::ContractReceiveChannelSettled(ref _inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveChannelDeposit(ref _inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveChannelWithdraw(ref _inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveChannelBatchUnlock(ref _inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        StateChange::ContractReceiveUpdateTransfer(ref _inner) => Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        }),
        _ => Err(StateTransitionError {
            msg: String::from("Could not transition channel"),
        }),
    }
}
