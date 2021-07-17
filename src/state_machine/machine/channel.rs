use std::cmp::min;

use web3::types::{
    Bytes,
    H256,
    U256,
};

use crate::{
    constants::{
        CHANNEL_STATES_PRIOR_TO_CLOSE,
        NUM_DISCRETISATION_POINTS,
    },
    errors::StateTransitionError,
    primitives::{
        FeeAmount,
        MediationFeeConfig,
        Random,
        TokenAmount,
        TransactionExecutionStatus,
        TransactionResult,
        U64,
    },
    state_machine::types::{
        Block,
        ChannelEndState,
        ChannelState,
        ChannelStatus,
        ContractReceiveChannelBatchUnlock,
        ContractReceiveChannelClosed,
        ContractReceiveChannelDeposit,
        ContractReceiveChannelSettled,
        ContractReceiveChannelWithdraw,
        ContractReceiveUpdateTransfer,
        ContractSendChannelBatchUnlock,
        ContractSendChannelSettle,
        ContractSendChannelUpdateTransfer,
        ContractSendEvent,
        Event,
        ExpiredWithdrawState,
        FeeScheduleState,
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

    let just_closed = state_change.canonical_identifier == channel_state.canonical_identifier
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

fn set_settled(mut channel_state: ChannelState, block_number: U64) -> ChannelState {
    if channel_state.settle_transaction.is_none() {
        channel_state.settle_transaction = Some(TransactionExecutionStatus {
            started_block_number: None,
            finished_block_number: Some(block_number),
            result: Some(TransactionResult::Success),
        });
    } else if let Some(ref mut settle_transaction) = channel_state.settle_transaction {
        if settle_transaction.finished_block_number.is_none() {
            settle_transaction.finished_block_number = Some(block_number);
            settle_transaction.result = Some(TransactionResult::Success);
        }
    }
    channel_state
}

fn handle_channel_settled(
    channel_state: ChannelState,
    state_change: ContractReceiveChannelSettled,
) -> TransitionResult {
    let mut events = vec![];

    if state_change.canonical_identifier == channel_state.canonical_identifier {
        let mut channel_state = set_settled(channel_state.clone(), state_change.block_number);
        let our_locksroot = state_change.our_onchain_locksroot.clone();
        let partner_locksroot = state_change.our_onchain_locksroot.clone();
        let should_clear_channel = our_locksroot == Bytes(vec![]) && partner_locksroot == Bytes(vec![]);

        if should_clear_channel {
            return Ok(ChannelTransition {
                new_state: None,
                events,
            });
        }

        channel_state.our_state.onchain_locksroot = our_locksroot;
        channel_state.partner_state.onchain_locksroot = partner_locksroot;

        events.push(Event::ContractSendChannelBatchUnlock(ContractSendChannelBatchUnlock {
            inner: ContractSendEvent {
                triggered_by_blockhash: state_change.block_hash,
            },
            canonical_identifier: channel_state.canonical_identifier,
            sender: channel_state.partner_state.address,
        }));
    }

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn update_contract_balance(end_state: &mut ChannelEndState, contract_balance: U256) {
    if contract_balance > end_state.contract_balance {
        end_state.contract_balance = contract_balance;
    }
}

/// Returns a list of num numbers from start to stop (inclusive).
fn linspace(start: u128, stop: u128, num: u128) -> Vec<TokenAmount> {
    // assert num > 1, "Must generate at least one step"
    // assert start <= stop, "start must be smaller than stop"

    let step = (stop - start) / (num - 1);

    let mut result = vec![];
    for i in 0..num {
        result.push(U256::from(start + i * step));
    }

    result
}

fn calculate_imbalance_fees(
    channel_capacity: U256,
    proportional_imbalance_fee: U256,
) -> Option<Vec<(TokenAmount, FeeAmount)>> {
    if proportional_imbalance_fee == U256::zero() {
        return None;
    }

    if channel_capacity == U256::zero() {
        return None;
    }

    let maximum_slope = U256::from(10 ^ -1);
    let max_imbalance_fee = channel_capacity.saturating_mul(proportional_imbalance_fee) / U256::from(1_000_000);

    // assert proportional_imbalance_fee / 1e6 <= maximum_slope / 2, "Too high imbalance fee"

    // calculate function parameters
    let s = maximum_slope;
    let c = max_imbalance_fee;
    let o = channel_capacity.div(2);
    let b = o.pow(s).div(c);
    let b = b.min(U256::from(10)); // limit exponent to keep numerical stability;
    let a = (c / o).pow(b);

    let f = |x: U256| -> U256 { a * (x - o).pow(b) };

    // calculate discrete function points
    let num_base_points = min(NUM_DISCRETISATION_POINTS.into(), channel_capacity + 1);
    let x_values: Vec<U256> = linspace(0, channel_capacity.as_u128(), num_base_points.as_u128());
    let y_values: Vec<U256> = x_values.iter().map(|x| f(*x)).collect();

    Some(x_values.into_iter().zip(y_values).collect())
}

fn update_fee_schedule_after_balance_change(channel_state: &mut ChannelState, fee_config: MediationFeeConfig) {
    let proportional_imbalance_fee = fee_config.get_proportional_imbalance_fee(&channel_state.token_address);
    let imbalance_penalty = calculate_imbalance_fees(channel_state.capacity(), proportional_imbalance_fee);

    channel_state.fee_schedule = FeeScheduleState {
        cap_fees: channel_state.fee_schedule.cap_fees,
        flat: channel_state.fee_schedule.flat,
        proportional: channel_state.fee_schedule.proportional,
        imbalance_penalty,
    }
}

fn handle_channel_deposit(
    mut channel_state: ChannelState,
    state_change: ContractReceiveChannelDeposit,
) -> TransitionResult {
    let participant_address = state_change.deposit_transaction.participant_address;
    let contract_balance = state_change.deposit_transaction.contract_balance;

    if participant_address == channel_state.our_state.address {
        update_contract_balance(&mut channel_state.our_state, contract_balance);
    } else if participant_address == channel_state.partner_state.address {
        update_contract_balance(&mut channel_state.partner_state, contract_balance);
    }

    update_fee_schedule_after_balance_change(&mut channel_state, state_change.fee_config);

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events: vec![],
    })
}

fn handle_channel_withdraw(
    mut channel_state: ChannelState,
    state_change: ContractReceiveChannelWithdraw,
) -> TransitionResult {
    if state_change.participant != channel_state.our_state.address
        && state_change.participant != channel_state.partner_state.address
    {
        return Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![],
        });
    }

    let end_state: &mut ChannelEndState = if state_change.participant == channel_state.our_state.address {
        &mut channel_state.our_state
    } else {
        &mut channel_state.partner_state
    };

    if let Some(_) = end_state.withdraws_pending.get(&state_change.total_withdraw) {
        end_state.withdraws_pending.remove(&state_change.total_withdraw);
    }
    end_state.onchain_total_withdraw = state_change.total_withdraw;

    update_fee_schedule_after_balance_change(&mut channel_state, state_change.fee_config);

    return Ok(ChannelTransition {
        new_state: Some(channel_state),
        events: vec![],
    });
}

fn handle_channel_batch_unlock(
    mut channel_state: ChannelState,
    state_change: ContractReceiveChannelBatchUnlock,
) -> TransitionResult {
    if channel_state.status() == ChannelStatus::Settled {
        if state_change.sender == channel_state.our_state.address {
            channel_state.our_state.onchain_locksroot = Bytes(vec![]);
        } else if state_change.sender == channel_state.partner_state.address {
            channel_state.partner_state.onchain_locksroot = Bytes(vec![]);
        }

        let no_unlocks_left_to_do = channel_state.our_state.onchain_locksroot == Bytes(vec![])
            && channel_state.partner_state.onchain_locksroot == Bytes(vec![]);
        if no_unlocks_left_to_do {
            return Ok(ChannelTransition {
                new_state: None,
                events: vec![],
            });
        }
    }

    return Ok(ChannelTransition {
        new_state: Some(channel_state),
        events: vec![],
    });
}

fn handle_channel_update_transfer(
    mut channel_state: ChannelState,
    state_change: ContractReceiveUpdateTransfer,
    block_number: U64,
) -> TransitionResult {
    if state_change.canonical_identifier == channel_state.canonical_identifier {
        channel_state.update_transaction = Some(TransactionExecutionStatus {
            started_block_number: None,
            finished_block_number: Some(block_number),
            result: Some(TransactionResult::Success),
        });
    }

    return Ok(ChannelTransition {
        new_state: Some(channel_state),
        events: vec![],
    });
}

pub fn state_transition(
    channel_state: ChannelState,
    state_change: StateChange,
    block_number: U64,
    _block_hash: H256,
    pseudo_random_number_generator: Random,
) -> TransitionResult {
    match state_change {
        StateChange::Block(inner) => handle_block(channel_state, inner, block_number, pseudo_random_number_generator),
        StateChange::ContractReceiveChannelClosed(inner) => handle_channel_closed(channel_state, inner),
        StateChange::ContractReceiveChannelSettled(inner) => handle_channel_settled(channel_state, inner),
        StateChange::ContractReceiveChannelDeposit(inner) => handle_channel_deposit(channel_state, inner),
        StateChange::ContractReceiveChannelWithdraw(inner) => handle_channel_withdraw(channel_state, inner),
        StateChange::ContractReceiveChannelBatchUnlock(inner) => handle_channel_batch_unlock(channel_state, inner),
        StateChange::ContractReceiveUpdateTransfer(inner) => {
            handle_channel_update_transfer(channel_state, inner, block_number)
        }
        _ => Err(StateTransitionError {
            msg: String::from("Could not transition channel"),
        }),
    }
}
