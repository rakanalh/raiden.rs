use crate::{
    constants::CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
    errors::StateTransitionError,
    primitives::{
        BlockHash,
        BlockNumber,
    },
    state_machine::{
        types::{
            ActionInitTarget,
            Block,
            ChainState,
            ChannelState,
            ContractReceiveSecretReveal,
            ErrorUnlockClaimFailed,
            Event,
            PaymentReceivedSuccess,
            ReceiveLockExpired,
            ReceiveSecretReveal,
            ReceiveUnlock,
            SendMessageEventInner,
            SendSecretRequest,
            SendSecretReveal,
            StateChange,
            TargetState,
            TargetTransferState,
        },
        views,
    },
};

use super::{
    channel::{
        self,
        get_address_metadata,
    },
    mediator,
    secret_registry,
    utils::{
        self,
        update_channel,
    },
};

pub(super) type TransitionResult = std::result::Result<TargetTransition, StateTransitionError>;

pub struct TargetTransition {
    pub new_state: Option<TargetTransferState>,
    pub chain_state: ChainState,
    pub events: Vec<Event>,
}

fn events_for_onchain_secretrevea(
    target_state: &mut TargetTransferState,
    channel_state: &ChannelState,
    block_number: BlockNumber,
    block_hash: BlockHash,
) -> Result<Vec<Event>, String> {
    let transfer = &target_state.transfer;
    let expiration = transfer.lock.expiration;

    let safe_to_wait = mediator::is_safe_to_wait(expiration, channel_state.reveal_timeout, block_number).is_ok();
    let secret_known_offchain =
        channel::is_secret_known_offchain(&channel_state.partner_state, transfer.lock.secrethash);
    let has_onchain_reveal_started = target_state.state == TargetState::OnchainSecretReveal;

    if !safe_to_wait && secret_known_offchain && !has_onchain_reveal_started {
        target_state.state = TargetState::OnchainSecretReveal;
        let secret = match channel_state.get_secret(&channel_state.partner_state, transfer.lock.secrethash) {
            Some(secret) => secret,
            None => {
                return Err("Secret should be known at this point".to_owned());
            }
        };

        return Ok(secret_registry::events_for_onchain_secretreveal(
            channel_state,
            secret,
            expiration,
            block_hash,
        ));
    }

    Ok(vec![])
}

fn handle_init_target(
    mut chain_state: ChainState,
    target_state: Option<TargetTransferState>,
    state_change: ActionInitTarget,
) -> TransitionResult {
    if target_state.is_some() {
        // Target state should be None
        return Ok(TargetTransition {
            new_state: target_state,
            chain_state,
            events: vec![],
        });
    }

    let transfer = &state_change.transfer;
    let from_hop = state_change.from_hop;

    let mut channel_state = match views::get_channel_by_canonical_identifier(
        &chain_state,
        transfer.balance_proof.canonical_identifier.clone(),
    ) {
        Some(channel_state) => channel_state.clone(),
        None => {
            return Ok(TargetTransition {
                new_state: None,
                chain_state,
                events: vec![],
            });
        }
    };

    let sender = match transfer.balance_proof.sender {
        Some(sender) => sender,
        None => return Err("Transfer sender should be set".to_owned().into()),
    };
    let handle_locked_transfer = channel::handle_receive_locked_transfer(
        &mut channel_state,
        transfer.clone(),
        get_address_metadata(sender, transfer.route_states.clone()),
    );
    let reveal_timeout = channel_state.reveal_timeout;
    update_channel(&mut chain_state, channel_state)?;

    let mut events = vec![];
    let target_state = match handle_locked_transfer {
        Ok(channel_event) => {
            // A valid balance proof does not mean the payment itself is still valid.
            // e.g. the lock may be near expiration or have expired. This is fine. The
            // message with an unusable lock must be handled to properly synchronize the
            // local view of the partner's channel state, allowing the next balance
            // proofs to be handled. This however, must only be done once, which is
            // enforced by the nonce increasing sequentially, which is verified by
            // the handler handle_receive_lockedtransfer.
            let target_state = TargetTransferState {
                from_hop,
                transfer: transfer.clone(),
                secret: None,
                state: TargetState::SecretRequest,
                initiator_address_metadata: None,
            };
            events.push(channel_event);

            if state_change.received_valid_secret {
                return Ok(TargetTransition {
                    new_state: Some(target_state),
                    chain_state,
                    events,
                });
            }

            let safe_to_wait =
                mediator::is_safe_to_wait(transfer.lock.expiration, reveal_timeout, chain_state.block_number).is_ok();
            if safe_to_wait {
                let message_identifier = chain_state.pseudo_random_number_generator.next();
                let recipient = transfer.initiator;
                let secret_request = SendSecretRequest {
                    inner: SendMessageEventInner {
                        recipient,
                        recipient_metadata: get_address_metadata(recipient, transfer.route_states.clone()),
                        canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
                        message_identifier,
                    },
                    payment_identifier: transfer.payment_identifier,
                    amount: transfer.lock.amount,
                    expiration: transfer.lock.expiration,
                    secrethash: transfer.lock.secrethash,
                };
                events.push(Event::SendSecretRequest(secret_request));
            }
            Some(target_state)
        }
        Err(e) => {
            let unlock_failed = Event::ErrorUnlockClaimFailed(ErrorUnlockClaimFailed {
                identifier: transfer.payment_identifier,
                secrethash: transfer.lock.secrethash,
                reason: e,
            });
            events.push(unlock_failed);
            None
        }
    };

    Ok(TargetTransition {
        new_state: target_state,
        chain_state,
        events,
    })
}

fn handle_block(
    chain_state: ChainState,
    target_state: Option<TargetTransferState>,
    state_change: Block,
) -> TransitionResult {
    let mut target_state = match target_state {
        Some(target_state) => target_state,
        None => return Err("Block should be accompanied by a valid target state".to_owned().into()),
    };

    let mut events = vec![];

    let transfer = &target_state.transfer;
    let lock = &transfer.lock;

    let channel_state = match views::get_channel_by_canonical_identifier(
        &chain_state,
        transfer.balance_proof.canonical_identifier.clone(),
    ) {
        Some(channel_state) => channel_state,
        None => {
            return Ok(TargetTransition {
                new_state: Some(target_state),
                chain_state,
                events: vec![],
            });
        }
    };

    let secret_known = channel::is_secret_known(&channel_state.partner_state, lock.secrethash);
    let lock_has_expired = channel::is_lock_expired(
        &channel_state.our_state,
        lock,
        chain_state.block_number,
        channel::get_receiver_expiration_threshold(lock.expiration),
    )
    .is_ok();

    if lock_has_expired && target_state.state != TargetState::Expired {
        target_state.state = TargetState::Expired;
        events.push(Event::ErrorUnlockClaimFailed(ErrorUnlockClaimFailed {
            identifier: transfer.payment_identifier,
            secrethash: transfer.lock.secrethash,
            reason: "Lock expired".to_owned(),
        }));
    } else if secret_known {
        events.extend(
            events_for_onchain_secretrevea(
                &mut target_state,
                &channel_state,
                state_change.block_number,
                state_change.block_hash,
            )
            .map_err(Into::into)?,
        );
    }

    Ok(TargetTransition {
        new_state: Some(target_state),
        chain_state,
        events,
    })
}

fn handle_offchain_secret_reveal(
    mut chain_state: ChainState,
    target_state: Option<TargetTransferState>,
    state_change: ReceiveSecretReveal,
) -> TransitionResult {
    let mut target_state = match target_state {
        Some(target_state) => target_state,
        None => return Err("Block should be accompanied by a valid target state".to_owned().into()),
    };

    let mut events = vec![];

    let transfer = &target_state.transfer;

    let mut channel_state = match views::get_channel_by_canonical_identifier(
        &chain_state,
        transfer.balance_proof.canonical_identifier.clone(),
    ) {
        Some(channel_state) => channel_state.clone(),
        None => {
            return Ok(TargetTransition {
                new_state: Some(target_state),
                chain_state,
                events: vec![],
            });
        }
    };

    let valid_secret = utils::is_valid_secret_reveal(&state_change, transfer.lock.secrethash);
    let has_transfer_expired = channel::is_transfer_expired(transfer, &channel_state, chain_state.block_number);

    if valid_secret && !has_transfer_expired {
        channel::register_offchain_secret(&mut channel_state, state_change.secret.clone(), state_change.secrethash);
        update_channel(&mut chain_state, channel_state)?;

        let from_hop = &target_state.from_hop;
        let message_identifier = chain_state.pseudo_random_number_generator.next();
        target_state.state = TargetState::OffchainSecretReveal;
        target_state.secret = Some(state_change.secret.clone());
        let recipient = from_hop.node_address;

        let reveal = SendSecretReveal {
            inner: SendMessageEventInner {
                recipient,
                recipient_metadata: get_address_metadata(recipient, transfer.route_states.clone()),
                canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
                message_identifier,
            },
            secret: state_change.secret,
            secrethash: state_change.secrethash,
        };

        events.push(Event::SendSecretReveal(reveal));
    }

    Ok(TargetTransition {
        new_state: Some(target_state),
        chain_state,
        events,
    })
}

fn handle_onchain_secret_reveal(
    mut chain_state: ChainState,
    target_state: Option<TargetTransferState>,
    state_change: ContractReceiveSecretReveal,
) -> TransitionResult {
    let mut target_state = match target_state {
        Some(target_state) => target_state,
        None => return Err("Block should be accompanied by a valid target state".to_owned().into()),
    };

    let transfer = &target_state.transfer;

    let mut channel_state = match views::get_channel_by_canonical_identifier(
        &chain_state,
        transfer.balance_proof.canonical_identifier.clone(),
    ) {
        Some(channel_state) => channel_state.clone(),
        None => {
            return Ok(TargetTransition {
                new_state: Some(target_state),
                chain_state,
                events: vec![],
            });
        }
    };

    let valid_secret = utils::is_valid_onchain_secret_reveal(&state_change, transfer.lock.secrethash);

    if valid_secret {
        channel::register_onchain_secret(
            &mut channel_state,
            state_change.secret.clone(),
            state_change.secrethash,
            state_change.block_number,
            true,
        );
        update_channel(&mut chain_state, channel_state)?;

        target_state.state = TargetState::OffchainSecretReveal;
        target_state.secret = Some(state_change.secret.clone());
    }

    Ok(TargetTransition {
        new_state: Some(target_state),
        chain_state,
        events: vec![],
    })
}

fn handle_lock_expired(
    mut chain_state: ChainState,
    target_state: Option<TargetTransferState>,
    state_change: ReceiveLockExpired,
) -> TransitionResult {
    let target_state = match target_state {
        Some(target_state) => target_state,
        None => return Err("Block should be accompanied by a valid target state".to_owned().into()),
    };

    let transfer = &target_state.transfer;

    let mut channel_state = match views::get_channel_by_canonical_identifier(
        &chain_state,
        transfer.balance_proof.canonical_identifier.clone(),
    ) {
        Some(channel_state) => channel_state.clone(),
        None => {
            return Ok(TargetTransition {
                new_state: Some(target_state),
                chain_state,
                events: vec![],
            });
        }
    };

    let sender = match transfer.balance_proof.sender {
        Some(sender) => sender,
        None => return Err("Transfer sender should be set".to_owned().into()),
    };
    let recipient_metadata = get_address_metadata(sender, transfer.route_states.clone());
    let mut result = channel::handle_receive_lock_expired(
        &mut channel_state,
        state_change,
        chain_state.block_number,
        recipient_metadata,
    )?;
    let channel_state = match result.new_state {
        Some(channel_state) => channel_state,
        None => {
            return Err("handle_receive_lock_expired should not delete channel"
                .to_owned()
                .into());
        }
    };

    update_channel(&mut chain_state, channel_state.clone())?;

    if channel::get_lock(&channel_state.partner_state, transfer.lock.secrethash).is_none() {
        let unlock_failed = Event::ErrorUnlockClaimFailed(ErrorUnlockClaimFailed {
            identifier: transfer.payment_identifier,
            secrethash: transfer.lock.secrethash,
            reason: "Lock expired".to_owned(),
        });
        result.events.push(unlock_failed);
    }

    Ok(TargetTransition {
        new_state: Some(target_state),
        chain_state,
        events: result.events,
    })
}

fn handle_unlock(
    mut chain_state: ChainState,
    target_state: Option<TargetTransferState>,
    state_change: ReceiveUnlock,
) -> TransitionResult {
    let target_state = match target_state {
        Some(target_state) => target_state,
        None => return Err("Block should be accompanied by a valid target state".to_owned().into()),
    };

    let mut events = vec![];
    let transfer = &target_state.transfer;

    let mut channel_state = match views::get_channel_by_canonical_identifier(
        &chain_state,
        transfer.balance_proof.canonical_identifier.clone(),
    ) {
        Some(channel_state) => channel_state.clone(),
        None => {
            return Ok(TargetTransition {
                new_state: Some(target_state),
                chain_state,
                events: vec![],
            });
        }
    };

    let sender = match transfer.balance_proof.sender {
        Some(sender) => sender,
        None => return Err("Transfer sender should be set".to_owned().into()),
    };
    let recipient_metadata = get_address_metadata(sender, transfer.route_states.clone());

    let unlock_event =
        channel::handle_unlock(&mut channel_state, state_change, recipient_metadata).map_err(Into::into)?;

    update_channel(&mut chain_state, channel_state.clone())?;

    let payment_received_success = Event::PaymentReceivedSuccess(PaymentReceivedSuccess {
        token_network_registry_address: channel_state.token_network_registry_address,
        token_network_address: channel_state.canonical_identifier.token_network_address,
        identifier: transfer.payment_identifier,
        amount: transfer.lock.amount,
        initiator: transfer.initiator,
    });
    events.push(unlock_event);
    events.push(payment_received_success);

    Ok(TargetTransition {
        new_state: None,
        chain_state,
        events,
    })
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
        StateChange::ActionInitTarget(inner) => handle_init_target(chain_state, target_state, inner),
        StateChange::Block(inner) => handle_block(chain_state, target_state, inner),
        StateChange::ReceiveSecretReveal(inner) => handle_offchain_secret_reveal(chain_state, target_state, inner),
        StateChange::ContractReceiveSecretReveal(inner) => {
            handle_onchain_secret_reveal(chain_state, target_state, inner)
        }
        StateChange::ReceiveUnlock(inner) => handle_unlock(chain_state, target_state, inner),
        StateChange::ReceiveLockExpired(inner) => handle_lock_expired(chain_state, target_state, inner),
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
