use std::{
    cmp::{
        max,
        min,
    },
    ops::{
        Div,
        Mul,
    },
};

use ethabi::{
    encode,
    ethereum_types::H256,
    Token,
};
use web3::{
    signing::{
        keccak256,
        recover,
    },
    types::{
        Address,
        Bytes,
        Recovery,
        U256,
    },
};

use crate::{
    constants::{
        CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
        CHANNEL_STATES_PRIOR_TO_CLOSE,
        DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS,
        LOCKSROOT_OF_NO_LOCKS,
        MAXIMUM_PENDING_TRANSFERS,
        NUM_DISCRETISATION_POINTS,
    },
    errors::StateTransitionError,
    primitives::{
        AddressMetadata,
        BalanceHash,
        BalanceProofData,
        BlockExpiration,
        BlockHash,
        BlockNumber,
        CanonicalIdentifier,
        FeeAmount,
        LockTimeout,
        LockedAmount,
        Locksroot,
        MessageHash,
        MessageIdentifier,
        Nonce,
        PaymentIdentifier,
        RevealTimeout,
        Secret,
        SecretHash,
        Signature,
        TokenAmount,
    },
    primitives::{
        MediationFeeConfig,
        Random,
        TransactionExecutionStatus,
        TransactionResult,
    },
    state_machine::{
        types::{
            ActionChannelClose,
            ActionChannelCoopSettle,
            ActionChannelSetRevealTimeout,
            ActionChannelWithdraw,
            BalanceProofState,
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
            ContractSendChannelClose,
            ContractSendChannelCoopSettle,
            ContractSendChannelSettle,
            ContractSendChannelUpdateTransfer,
            ContractSendChannelWithdraw,
            ContractSendEventInner,
            CoopSettleState,
            ErrorInvalidActionCoopSettle,
            ErrorInvalidActionSetRevealTimeout,
            ErrorInvalidActionWithdraw,
            ErrorInvalidReceivedLockExpired,
            ErrorInvalidReceivedLockedTransfer,
            ErrorInvalidReceivedTransferRefund,
            ErrorInvalidReceivedUnlock,
            ErrorInvalidReceivedWithdrawConfirmation,
            ErrorInvalidReceivedWithdrawExpired,
            ErrorInvalidReceivedWithdrawRequest,
            Event,
            ExpiredWithdrawState,
            FeeScheduleState,
            HashTimeLockState,
            LockedTransferState,
            PendingLocksState,
            PendingWithdrawState,
            ReceiveLockExpired,
            ReceiveTransferRefund,
            ReceiveUnlock,
            ReceiveWithdrawConfirmation,
            ReceiveWithdrawExpired,
            ReceiveWithdrawRequest,
            RouteState,
            SendLockExpired,
            SendLockedTransfer,
            SendMessageEventInner,
            SendProcessed,
            SendUnlock,
            SendWithdrawConfirmation,
            SendWithdrawExpired,
            SendWithdrawRequest,
            StateChange,
            UnlockPartialProofState,
        },
        views,
    },
};

type TransitionResult = std::result::Result<ChannelTransition, StateTransitionError>;

pub struct ChannelTransition {
    pub new_state: Option<ChannelState>,
    pub events: Vec<Event>,
}

pub fn balance(sender: &ChannelEndState, receiver: &ChannelEndState, subtract_withdraw: bool) -> TokenAmount {
    let mut sender_transferred_amount = TokenAmount::zero();
    let mut receiver_transferred_amount = TokenAmount::zero();

    if let Some(sender_balance_proof) = &sender.balance_proof {
        sender_transferred_amount = sender_balance_proof.transferred_amount;
    }

    if let Some(receiver_balance_proof) = &receiver.balance_proof {
        receiver_transferred_amount = receiver_balance_proof.transferred_amount;
    }

    let max_withdraw = max(sender.offchain_total_withdraw(), sender.onchain_total_withdraw);
    let withdraw = if subtract_withdraw {
        max_withdraw
    } else {
        TokenAmount::zero()
    };

    sender.contract_balance - withdraw - sender_transferred_amount + receiver_transferred_amount
}

/// Calculates the maximum "total_withdraw_amount" for a channel.
/// This will leave the channel without funds, when this is withdrawn from the contract,
/// or pending as offchain-withdraw.
fn get_max_withdraw_amount(sender_state: &ChannelEndState, receiver_state: &ChannelEndState) -> TokenAmount {
    balance(sender_state, receiver_state, false)
}

pub(super) fn get_address_metadata(
    recipient_address: Address,
    route_states: Vec<RouteState>,
) -> Option<AddressMetadata> {
    for route_state in route_states {
        match route_state.address_to_metadata.get(&recipient_address) {
            Some(metadata) => return Some(metadata.clone()),
            None => continue,
        };
    }

    None
}

pub(super) fn get_safe_initial_expiration(
    block_number: BlockNumber,
    reveal_timeout: RevealTimeout,
    lock_timeout: Option<LockTimeout>,
) -> BlockNumber {
    if let Some(lock_timeout) = lock_timeout {
        return block_number + lock_timeout;
    }

    block_number + (reveal_timeout * 2)
}

pub(super) fn is_secret_known(end_state: &ChannelEndState, secrethash: SecretHash) -> bool {
    end_state.secrethashes_to_unlockedlocks.contains_key(&secrethash)
        || end_state
            .secrethashes_to_onchain_unlockedlocks
            .contains_key(&secrethash)
}

pub(super) fn is_secret_known_offchain(end_state: &ChannelEndState, secrethash: SecretHash) -> bool {
    end_state.secrethashes_to_unlockedlocks.contains_key(&secrethash)
}

#[allow(dead_code)]
pub(super) fn is_secret_known_onchain(end_state: &ChannelEndState, secrethash: SecretHash) -> bool {
    end_state
        .secrethashes_to_onchain_unlockedlocks
        .contains_key(&secrethash)
}

pub(super) fn is_lock_expired(
    end_state: &ChannelEndState,
    lock: &HashTimeLockState,
    block_number: BlockNumber,
    lock_expiration_threshold: BlockExpiration,
) -> Result<(), String> {
    let secret_registered_on_chain = end_state
        .secrethashes_to_onchain_unlockedlocks
        .get(&lock.secrethash)
        .is_some();

    if secret_registered_on_chain {
        return Err("Lock has been unlocked onchain".to_owned());
    }

    if block_number < lock_expiration_threshold {
        return Err(format!(
            "Current block number ({}) is not larger than \
             lock.expiration + confirmation blocks ({})",
            block_number, lock_expiration_threshold
        ));
    }

    Ok(())
}

pub(super) fn is_lock_pending(end_state: &ChannelEndState, secrethash: SecretHash) -> bool {
    end_state.secrethashes_to_lockedlocks.contains_key(&secrethash)
        || end_state.secrethashes_to_unlockedlocks.contains_key(&secrethash)
        || end_state
            .secrethashes_to_onchain_unlockedlocks
            .contains_key(&secrethash)
}

pub(super) fn is_lock_locked(end_state: &ChannelEndState, secrethash: SecretHash) -> bool {
    end_state.secrethashes_to_lockedlocks.contains_key(&secrethash)
}

fn create_send_expired_lock(
    sender_end_state: &mut ChannelEndState,
    locked_lock: HashTimeLockState,
    pseudo_random_number_generator: &mut Random,
    canonical_identifier: CanonicalIdentifier,
    recipient: Address,
    recipient_metadata: Option<AddressMetadata>,
) -> Result<(Option<SendLockExpired>, Option<PendingLocksState>), String> {
    let locked_amount = get_amount_locked(&sender_end_state);
    let balance_proof = match &sender_end_state.balance_proof {
        Some(bp) => bp.clone(),
        None => {
            return Ok((None, None));
        }
    };
    let updated_locked_amount = locked_amount - locked_lock.amount;
    let transferred_amount = balance_proof.transferred_amount;
    let secrethash = locked_lock.secrethash;
    let pending_locks = match compute_locks_without(&mut sender_end_state.pending_locks, &locked_lock) {
        Some(locks) => locks,
        None => {
            return Ok((None, None));
        }
    };

    let nonce = get_next_nonce(&sender_end_state);
    let locksroot = compute_locksroot(&pending_locks);
    let balance_hash = hash_balance_data(transferred_amount, locked_amount, locksroot.clone())?;
    let balance_proof = BalanceProofState {
        nonce,
        transferred_amount,
        locksroot,
        balance_hash,
        canonical_identifier: canonical_identifier.clone(),
        locked_amount: updated_locked_amount,
        message_hash: None,
        signature: None,
        sender: None,
    };
    let send_lock_expired = SendLockExpired {
        inner: SendMessageEventInner {
            recipient,
            recipient_metadata,
            canonical_identifier,
            message_identifier: pseudo_random_number_generator.next(),
        },
        balance_proof,
        secrethash,
    };

    Ok((Some(send_lock_expired), Some(pending_locks)))
}

fn delete_unclaimed_lock(end_state: &mut ChannelEndState, secrethash: SecretHash) {
    if end_state.secrethashes_to_lockedlocks.contains_key(&secrethash) {
        end_state.secrethashes_to_lockedlocks.remove(&secrethash);
    }

    if end_state.secrethashes_to_unlockedlocks.contains_key(&secrethash) {
        end_state.secrethashes_to_unlockedlocks.remove(&secrethash);
    }
}

fn delete_lock(end_state: &mut ChannelEndState, secrethash: SecretHash) {
    delete_unclaimed_lock(end_state, secrethash);

    if end_state
        .secrethashes_to_onchain_unlockedlocks
        .contains_key(&secrethash)
    {
        end_state.secrethashes_to_onchain_unlockedlocks.remove(&secrethash);
    }
}

pub(super) fn get_lock(end_state: &ChannelEndState, secrethash: SecretHash) -> Option<HashTimeLockState> {
    let mut lock = end_state.secrethashes_to_lockedlocks.get(&secrethash);
    if lock.is_none() {
        lock = end_state
            .secrethashes_to_unlockedlocks
            .get(&secrethash)
            .map(|lock| &lock.lock);
    }
    if lock.is_none() {
        lock = end_state
            .secrethashes_to_onchain_unlockedlocks
            .get(&secrethash)
            .map(|lock| &lock.lock);
    }
    lock.cloned()
}

/// Check if the lock with `secrethash` exists in either our state or the partner's state"""
pub(super) fn lock_exists_in_either_channel_side(channel_state: &ChannelState, secrethash: SecretHash) -> bool {
    let lock_exists = |end_state: &ChannelEndState, secrethash: SecretHash| {
        if end_state.secrethashes_to_lockedlocks.get(&secrethash).is_some() {
            return true;
        }
        if end_state.secrethashes_to_unlockedlocks.get(&secrethash).is_some() {
            return true;
        }
        if end_state
            .secrethashes_to_onchain_unlockedlocks
            .get(&secrethash)
            .is_some()
        {
            return true;
        }
        false
    };
    lock_exists(&channel_state.our_state, secrethash) || lock_exists(&channel_state.partner_state, secrethash)
}

pub(super) fn send_lock_expired(
    mut channel_state: ChannelState,
    locked_lock: HashTimeLockState,
    pseudo_random_number_generator: &mut Random,
    recipient_metadata: Option<AddressMetadata>,
) -> Result<(ChannelState, Vec<SendLockExpired>), String> {
    if channel_state.status() != ChannelStatus::Opened {
        return Ok((channel_state, vec![]));
    }

    let secrethash = locked_lock.secrethash.clone();
    let (send_lock_expired, pending_locks) = create_send_expired_lock(
        &mut channel_state.our_state,
        locked_lock,
        pseudo_random_number_generator,
        channel_state.canonical_identifier.clone(),
        channel_state.partner_state.address,
        recipient_metadata,
    )?;

    let events = if let (Some(send_lock_expired), Some(pending_locks)) = (send_lock_expired, pending_locks) {
        channel_state.our_state.pending_locks = pending_locks;
        channel_state.our_state.balance_proof = Some(send_lock_expired.balance_proof.clone());
        channel_state.our_state.nonce = send_lock_expired.balance_proof.nonce;

        delete_unclaimed_lock(&mut channel_state.our_state, secrethash);

        vec![send_lock_expired]
    } else {
        vec![]
    };

    Ok((channel_state, events))
}

fn create_unlock(
    channel_state: &mut ChannelState,
    message_identifier: MessageIdentifier,
    payment_identifier: PaymentIdentifier,
    secret: Secret,
    lock: &HashTimeLockState,
    block_number: BlockNumber,
    recipient_metadata: Option<AddressMetadata>,
) -> Result<(SendUnlock, PendingLocksState), String> {
    if channel_state.status() == ChannelStatus::Opened {
        return Err("Channel is not open".to_owned());
    }

    if !is_lock_pending(&channel_state.our_state, lock.secrethash) {
        return Err("Lock expired".to_owned());
    }

    let expired = is_lock_expired(&channel_state.our_state, &lock, block_number, lock.expiration).is_ok();
    if expired {
        return Err("Lock expired".to_owned());
    }

    let our_balance_proof = match &channel_state.our_state.balance_proof {
        Some(balance_proof) => balance_proof,
        None => {
            return Err("No transfers exist on our state".to_owned());
        }
    };

    let transferred_amount = lock.amount + our_balance_proof.transferred_amount;
    let pending_locks = match compute_locks_without(&mut channel_state.our_state.pending_locks, &lock) {
        Some(pending_locks) => pending_locks,
        None => {
            return Err("Lock is pending, it must be in the pending locks".to_owned());
        }
    };

    let locksroot = compute_locksroot(&pending_locks);
    let token_address = channel_state.token_address;
    let recipient = channel_state.partner_state.address;
    let locked_amount = get_amount_locked(&channel_state.our_state) - lock.amount;
    let nonce = get_next_nonce(&channel_state.our_state);
    channel_state.our_state.nonce = nonce;

    let balance_hash = hash_balance_data(transferred_amount, locked_amount, locksroot.clone())?;

    let balance_proof = BalanceProofState {
        nonce,
        transferred_amount,
        locked_amount,
        locksroot,
        balance_hash,
        canonical_identifier: channel_state.canonical_identifier.clone(),
        message_hash: None,
        signature: None,
        sender: None,
    };

    let unlock_lock = SendUnlock {
        inner: SendMessageEventInner {
            recipient,
            recipient_metadata,
            message_identifier,
            canonical_identifier: channel_state.canonical_identifier.clone(),
        },
        payment_identifier,
        token_address,
        balance_proof,
        secret,
        secrethash: lock.secrethash,
    };

    Ok((unlock_lock, pending_locks))
}

pub(super) fn send_unlock(
    channel_state: &mut ChannelState,
    message_identifier: MessageIdentifier,
    payment_identifier: PaymentIdentifier,
    secret: Secret,
    secrethash: SecretHash,
    block_number: BlockNumber,
    recipient_metadata: Option<AddressMetadata>,
) -> Result<SendUnlock, String> {
    let lock = match get_lock(&channel_state.our_state, secrethash) {
        Some(lock) => lock,
        None => {
            return Err("Caller must ensure the lock exists".to_owned());
        }
    };

    let (unlock, pending_locks) = create_unlock(
        channel_state,
        message_identifier,
        payment_identifier,
        secret,
        &lock,
        block_number,
        recipient_metadata,
    )?;

    channel_state.our_state.balance_proof = Some(unlock.balance_proof.clone());
    channel_state.our_state.pending_locks = pending_locks;

    delete_lock(&mut channel_state.our_state, lock.secrethash);

    Ok(unlock)
}

pub(super) fn handle_unlock(
    channel_state: &mut ChannelState,
    unlock: ReceiveUnlock,
    recipient_metadata: Option<AddressMetadata>,
) -> Result<Event, String> {
    Ok(
        match is_valid_unlock(&channel_state.clone(), &mut channel_state.partner_state, unlock.clone()) {
            Ok(pending_locks) => {
                channel_state.partner_state.balance_proof = Some(unlock.balance_proof.clone());
                channel_state.partner_state.nonce = unlock.balance_proof.nonce;
                channel_state.partner_state.pending_locks = pending_locks;

                delete_lock(&mut channel_state.partner_state, unlock.secrethash);

                SendProcessed {
                    inner: SendMessageEventInner {
                        recipient: unlock.balance_proof.sender.expect("Should exist"),
                        recipient_metadata,
                        canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
                        message_identifier: unlock.message_identifier,
                    },
                }
                .into()
            }
            Err(e) => ErrorInvalidReceivedUnlock {
                secrethash: unlock.secrethash,
                reason: e,
            }
            .into(),
        },
    )
}

fn register_onchain_secret_endstate(
    end_state: &mut ChannelEndState,
    secret: Secret,
    secrethash: SecretHash,
    secret_reveal_block_number: BlockNumber,
    should_delete_lock: bool,
) {
    let mut pending_lock = None;
    if is_lock_locked(end_state, secrethash) {
        pending_lock = end_state.secrethashes_to_lockedlocks.get_mut(&secrethash);
    }

    if let Some(lock) = end_state.secrethashes_to_unlockedlocks.get_mut(&secrethash) {
        pending_lock = Some(&mut lock.lock);
    }

    if let Some(lock) = pending_lock {
        if lock.expiration < secret_reveal_block_number {
            return;
        }

        end_state.secrethashes_to_onchain_unlockedlocks.insert(
            secrethash,
            UnlockPartialProofState {
                secret,
                secrethash,
                lock: lock.clone(),
                amount: lock.amount,
                expiration: lock.expiration,
                encoded: lock.encoded.clone(),
            },
        );

        if should_delete_lock {
            delete_lock(end_state, secrethash);
        }
    }
}

pub(super) fn register_onchain_secret(
    channel_state: &mut ChannelState,
    secret: Secret,
    secrethash: SecretHash,
    secret_reveal_block_number: BlockNumber,
    should_delete_lock: bool,
) {
    register_onchain_secret_endstate(
        &mut channel_state.our_state,
        secret.clone(),
        secrethash,
        secret_reveal_block_number,
        should_delete_lock,
    );
    register_onchain_secret_endstate(
        &mut channel_state.partner_state,
        secret,
        secrethash,
        secret_reveal_block_number,
        should_delete_lock,
    );
}

fn hash_balance_data(
    transferred_amount: TokenAmount,
    locked_amount: LockedAmount,
    locksroot: Locksroot,
) -> Result<BalanceHash, String> {
    if locksroot == Bytes(vec![]) {
        return Err("Can't hash empty locksroot".to_string());
    }

    if locksroot.0.len() != 32 {
        return Err("Locksroot has wrong length".to_string());
    }

    let mut transferred_amount_in_bytes = vec![];
    transferred_amount.to_big_endian(&mut transferred_amount_in_bytes);

    let mut locked_amount_in_bytes = vec![];
    locked_amount.to_big_endian(&mut locked_amount_in_bytes);

    let hash = keccak256(
        &[
            &transferred_amount_in_bytes[..],
            &locked_amount_in_bytes[..],
            &locksroot.0[..],
        ]
        .concat(),
    );
    Ok(H256::from_slice(&hash))
}

fn pack_balance_proof(
    nonce: Nonce,
    balance_hash: BalanceHash,
    additional_hash: MessageHash,
    canonical_identifier: CanonicalIdentifier,
) -> Bytes {
    let mut b = vec![];

    b.extend(encode(&[
        Token::Address(canonical_identifier.token_network_address),
        Token::Uint(canonical_identifier.chain_identifier.into()),
        Token::Uint(canonical_identifier.channel_identifier),
    ]));
    b.extend(balance_hash.as_bytes());
    b.extend(encode(&[Token::Uint(nonce)]));
    b.extend(additional_hash.as_bytes());

    Bytes(b)
}

fn pack_withdraw(
    canonical_identifier: CanonicalIdentifier,
    participant: Address,
    total_withdraw: TokenAmount,
    expiration_block: BlockExpiration,
) -> Bytes {
    let mut b = vec![];

    b.extend(encode(&[
        Token::Address(canonical_identifier.token_network_address),
        Token::Uint(canonical_identifier.chain_identifier.into()),
        Token::Uint(canonical_identifier.channel_identifier),
        Token::Address(participant),
        Token::Uint(total_withdraw),
        Token::Uint(expiration_block.into()),
    ]));

    Bytes(b)
}

fn get_next_nonce(end_state: &ChannelEndState) -> Nonce {
    end_state.nonce + 1
}

fn get_amount_locked(end_state: &ChannelEndState) -> LockedAmount {
    let total_pending: TokenAmount = end_state
        .secrethashes_to_lockedlocks
        .values()
        .map(|lock| lock.amount)
        .fold(U256::zero(), |acc, x| acc.saturating_add(x));
    let total_unclaimed: TokenAmount = end_state
        .secrethashes_to_unlockedlocks
        .values()
        .map(|lock| lock.amount)
        .fold(U256::zero(), |acc, x| acc.saturating_add(x));
    let total_unclaimed_onchain = end_state
        .secrethashes_to_onchain_unlockedlocks
        .values()
        .map(|lock| lock.amount)
        .fold(U256::zero(), |acc, x| acc.saturating_add(x));

    total_pending + total_unclaimed + total_unclaimed_onchain
}

pub(super) fn refund_transfer_matches_transfer(
    refund_transfer: &LockedTransferState,
    transfer: &LockedTransferState,
) -> bool {
    if let Some(sender) = refund_transfer.balance_proof.sender {
        if sender == transfer.target {
            return false;
        }
    }

    transfer.payment_identifier == refund_transfer.payment_identifier
        && transfer.lock.amount == refund_transfer.lock.amount
        && transfer.lock.secrethash == refund_transfer.lock.secrethash
        && transfer.target == refund_transfer.target
        && transfer.lock.expiration == refund_transfer.lock.expiration
        && transfer.token == refund_transfer.token
}

fn compute_locks_with(pending_locks: &mut PendingLocksState, lock: HashTimeLockState) -> Option<PendingLocksState> {
    if !pending_locks.locks.contains(&lock.encoded) {
        let mut locks = PendingLocksState {
            locks: pending_locks.locks.clone(),
        };
        locks.locks.push(lock.encoded);
        return Some(locks);
    }

    None
}

fn compute_locks_without(pending_locks: &mut PendingLocksState, lock: &HashTimeLockState) -> Option<PendingLocksState> {
    if pending_locks.locks.contains(&lock.encoded) {
        let mut locks = PendingLocksState {
            locks: pending_locks.locks.clone(),
        };
        locks.locks.retain(|l| l != &lock.encoded);
        return Some(locks);
    }

    None
}

fn compute_locksroot(locks: &PendingLocksState) -> Locksroot {
    let locks: Vec<&[u8]> = locks.locks.iter().map(|lock| lock.0.as_slice()).collect();
    let hash = keccak256(&locks.concat());
    return Bytes(hash.to_vec());
}

fn create_locked_transfer(
    channel_state: &mut ChannelState,
    initiator: Address,
    target: Address,
    amount: TokenAmount,
    expiration: BlockExpiration,
    secrethash: SecretHash,
    message_identifier: MessageIdentifier,
    payment_identifier: PaymentIdentifier,
    route_states: Vec<RouteState>,
    recipient_metadata: Option<AddressMetadata>,
) -> Result<(SendLockedTransfer, PendingLocksState), String> {
    if amount > channel_state.get_distributable(&channel_state.our_state, &channel_state.partner_state) {
        return Err("Caller must make sure there is enough balance".to_string());
    }

    if channel_state.status() != ChannelStatus::Opened {
        return Err("Caller must make sure the channel is open".to_string());
    }

    let lock = HashTimeLockState::create(amount, expiration, secrethash);
    let pending_locks = match compute_locks_with(&mut channel_state.our_state.pending_locks, lock.clone()) {
        Some(pending_locks) => pending_locks,
        None => {
            return Err("Caller must make sure the lock isn't used twice".to_string());
        }
    };

    let locksroot = compute_locksroot(&pending_locks);

    let transferred_amount = if let Some(our_balance_proof) = &channel_state.our_state.balance_proof {
        our_balance_proof.transferred_amount
    } else {
        TokenAmount::zero()
    };

    if transferred_amount.checked_add(amount).is_none() {
        return Err("Caller must make sure the result wont overflow".to_string());
    }

    let token = channel_state.token_address;
    let locked_amount = get_amount_locked(&channel_state.our_state) + amount;
    let nonce = get_next_nonce(&channel_state.our_state);
    let balance_hash = hash_balance_data(amount, locked_amount, locksroot.clone())?;
    let balance_proof = BalanceProofState {
        nonce,
        transferred_amount,
        locked_amount,
        locksroot,
        balance_hash,
        canonical_identifier: channel_state.canonical_identifier.clone(),
        message_hash: None,
        signature: None,
        sender: None,
    };

    let locked_transfer = LockedTransferState {
        payment_identifier,
        token,
        lock,
        initiator,
        target,
        message_identifier,
        balance_proof,
        route_states: route_states.clone(),
    };

    let recipient = channel_state.partner_state.address;
    let recipient_metadata = match recipient_metadata {
        Some(metadata) => Some(metadata),
        None => get_address_metadata(recipient, route_states),
    };
    let locked_transfer_event = SendLockedTransfer {
        inner: SendMessageEventInner {
            recipient,
            recipient_metadata,
            canonical_identifier: channel_state.canonical_identifier.clone(),
            message_identifier,
        },
        transfer: locked_transfer,
    };

    Ok((locked_transfer_event, pending_locks))
}

pub(super) fn send_locked_transfer(
    mut channel_state: ChannelState,
    initiator: Address,
    target: Address,
    amount: TokenAmount,
    expiration: BlockExpiration,
    secrethash: SecretHash,
    message_identifier: MessageIdentifier,
    payment_identifier: PaymentIdentifier,
    route_states: Vec<RouteState>,
    recipient_metadata: Option<AddressMetadata>,
) -> Result<(ChannelState, SendLockedTransfer), String> {
    let (locked_transfer, pending_locks) = create_locked_transfer(
        &mut channel_state,
        initiator,
        target,
        amount,
        expiration,
        secrethash,
        message_identifier,
        payment_identifier,
        route_states,
        recipient_metadata,
    )?;

    let transfer = locked_transfer.transfer.clone();
    let lock = transfer.lock.clone();
    channel_state.our_state.balance_proof = Some(transfer.balance_proof.clone());
    channel_state.our_state.nonce = transfer.balance_proof.nonce;
    channel_state.our_state.pending_locks = pending_locks.clone();
    channel_state
        .our_state
        .secrethashes_to_lockedlocks
        .insert(lock.secrethash, lock);

    Ok((channel_state, locked_transfer))
}

fn send_expired_withdraws(
    mut channel_state: ChannelState,
    block_number: BlockNumber,
    pseudo_random_number_generator: &mut Random,
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

        events.push(
            SendWithdrawExpired {
                inner: SendMessageEventInner {
                    recipient: channel_state.partner_state.address,
                    recipient_metadata: withdraw_state.recipient_metadata.clone(),
                    canonical_identifier: channel_state.canonical_identifier.clone(),
                    message_identifier: pseudo_random_number_generator.next(),
                },
                participant: channel_state.our_state.address,
                total_withdraw: withdraw_state.total_withdraw,
                nonce: channel_state.our_state.nonce,
                expiration: withdraw_state.expiration,
            }
            .into(),
        );
    }

    events
}

fn get_current_balance_proof(end_state: &ChannelEndState) -> BalanceProofData {
    if let Some(balance_proof) = &end_state.balance_proof {
        (
            balance_proof.locksroot.clone(),
            end_state.nonce,
            balance_proof.transferred_amount,
            get_amount_locked(end_state),
        )
    } else {
        (
            Bytes(LOCKSROOT_OF_NO_LOCKS.to_vec()),
            Nonce::zero(),
            TokenAmount::zero(),
            LockedAmount::zero(),
        )
    }
}

fn is_valid_signature(data: Bytes, signature: Signature, sender_address: Address) -> Result<(), String> {
    let recovery = Recovery::from_raw_signature(data.0.as_slice(), signature).map_err(|e| e.to_string())?;
    let recovery_id = match recovery.recovery_id() {
        Some(id) => id,
        None => return Err("Found invalid recovery ID".to_owned()),
    };
    let signer_address = recover(data.0.as_slice(), signature.as_bytes(), recovery_id)
        .map_err(|e| format!("Error recovering signature {:?}", e))?;

    if signer_address == sender_address {
        return Ok(());
    }

    return Err("Signature was valid but the expected address does not match".to_owned());
}

fn is_valid_balance_proof_signature(balance_proof: &BalanceProofState, sender_address: Address) -> Result<(), String> {
    let balance_hash = hash_balance_data(
        balance_proof.transferred_amount,
        balance_proof.locked_amount,
        balance_proof.locksroot.clone(),
    )?;
    let message_hash = match balance_proof.message_hash {
        Some(hash) => hash,
        None => MessageHash::zero(),
    };
    let data_that_was_signed = pack_balance_proof(
        balance_proof.nonce,
        balance_hash,
        message_hash,
        balance_proof.canonical_identifier.clone(),
    );

    let signature = match balance_proof.signature {
        Some(signature) => signature,
        None => {
            return Err("Balance proof must be signed".to_owned());
        }
    };

    is_valid_signature(data_that_was_signed, signature, sender_address)
}

fn is_balance_proof_safe_for_onchain_operations(balance_proof: &BalanceProofState) -> bool {
    balance_proof
        .transferred_amount
        .checked_add(balance_proof.locked_amount)
        .is_some()
}

pub(super) fn is_transfer_expired(
    transfer: &LockedTransferState,
    affected_channel: &ChannelState,
    block_number: BlockNumber,
) -> bool {
    let lock_expiration_threshold = get_sender_expiration_threshold(transfer.lock.expiration);

    is_lock_expired(
        &affected_channel.our_state,
        &transfer.lock,
        block_number,
        lock_expiration_threshold,
    )
    .is_ok()
}

fn is_balance_proof_usable_onchain(
    received_balance_proof: &BalanceProofState,
    channel_state: &ChannelState,
    sender_state: &ChannelEndState,
) -> Result<(), String> {
    let expected_nonce = get_next_nonce(sender_state);

    let is_valid_signature = is_valid_balance_proof_signature(&received_balance_proof, sender_state.address);

    if channel_state.status() != ChannelStatus::Opened {
        return Err("The channel is already closed.".to_owned());
    } else if received_balance_proof.canonical_identifier != channel_state.canonical_identifier {
        return Err("Canonical identifier does not match".to_owned());
    } else if !is_balance_proof_safe_for_onchain_operations(&received_balance_proof) {
        return Err("Balance proof total transferred amount would overflow onchain.".to_owned());
    } else if received_balance_proof.nonce != expected_nonce {
        return Err(format!(
            "Nonce did not change sequentially. \
                            Expected: {} \
                            got: {}",
            expected_nonce, received_balance_proof.nonce
        ));
    }
    is_valid_signature
}

pub(super) fn get_sender_expiration_threshold(expiration: BlockExpiration) -> BlockExpiration {
    expiration + DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS.mul(2).into()
}

pub(super) fn get_receiver_expiration_threshold(expiration: BlockExpiration) -> BlockExpiration {
    expiration + DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS.into()
}

fn is_valid_lock_expired(
    channel_state: &ChannelState,
    state_change: ReceiveLockExpired,
    sender_state: &ChannelEndState,
    receiver_state: &ChannelEndState,
    block_number: BlockNumber,
) -> Result<PendingLocksState, String> {
    let secrethash = state_change.secrethash;
    let received_balance_proof = state_change.balance_proof;
    let lock = channel_state
        .partner_state
        .secrethashes_to_lockedlocks
        .get(&secrethash)
        .or_else(|| {
            channel_state
                .partner_state
                .secrethashes_to_unlockedlocks
                .get(&secrethash)
                .map(|lock| &lock.lock)
        });

    let secret_registered_on_chain = channel_state
        .partner_state
        .secrethashes_to_onchain_unlockedlocks
        .contains_key(&secrethash);
    let (_, _, current_transferred_amount, current_locked_amount) = get_current_balance_proof(sender_state);
    let is_valid_balance_proof = is_balance_proof_usable_onchain(&received_balance_proof, channel_state, sender_state);

    let (lock, expected_locked_amount) = match lock {
        Some(lock) => {
            let expected_locked_amount = current_locked_amount - lock.amount;
            (lock, expected_locked_amount)
        }
        None => {
            return Err(format!(
                "Invalid LockExpired message. \
                                Lock with secrethash {} is not known",
                secrethash
            ));
        }
    };
    let pending_locks = match compute_locks_without(&mut sender_state.pending_locks.clone(), lock) {
        Some(pending_locks) => pending_locks,
        None => {
            return Err(format!("Invalid LockExpired message. Same lock handled twice."));
        }
    };

    if secret_registered_on_chain {
        return Err(format!("Invalid LockExpired message. Lock was unlocked on-chain"));
    } else if let Err(e) = is_valid_balance_proof {
        return Err(format!("Invalid LockExpired message. {}", e));
    }

    let locksroot_without_lock = compute_locksroot(&pending_locks);
    let check_lock_expired = is_lock_expired(
        receiver_state,
        lock,
        block_number,
        get_receiver_expiration_threshold(lock.expiration),
    );

    if let Err(e) = check_lock_expired {
        return Err(format!("Invalid LockExpired message. {}", e));
    } else if received_balance_proof.locksroot != locksroot_without_lock {
        return Err(format!(
            "Invalid LockExpired message. \
                            Balance proof's locksroot didn't match. \
                            expected {:?} \
                            got {:?}",
            locksroot_without_lock, received_balance_proof.locksroot
        ));
    } else if received_balance_proof.transferred_amount != current_transferred_amount {
        return Err(format!(
            "Invalid LockExpired message. \
                            Balance proof's transferred amount changed. \
                            expected {} \
                            got {}",
            current_transferred_amount, received_balance_proof.transferred_amount
        ));
    } else if received_balance_proof.locked_amount != expected_locked_amount {
        return Err(format!(
            "Invalid LockExpired message. \
                            Balance proof's locked amount changed. \
                            expected {} \
                            got {}",
            expected_locked_amount, received_balance_proof.locked_amount
        ));
    }

    Ok(pending_locks)
}

fn valid_locked_transfer_check(
    channel_state: &ChannelState,
    sender_state: &mut ChannelEndState,
    receiver_state: &ChannelEndState,
    message: &'static str,
    received_balance_proof: &BalanceProofState,
    lock: &HashTimeLockState,
) -> Result<PendingLocksState, String> {
    let (_, _, current_transferred_amount, current_locked_amount) = get_current_balance_proof(sender_state);
    let distributable = channel_state.get_distributable(sender_state, receiver_state);
    let expected_locked_amount = current_locked_amount + lock.amount;

    if let Err(e) = is_balance_proof_usable_onchain(&received_balance_proof, channel_state, sender_state) {
        return Err(format!("Invalid {} message. {}", message, e));
    }

    let pending_locks = match compute_locks_with(&mut sender_state.pending_locks, lock.clone()) {
        Some(pending_locks) => {
            if pending_locks.locks.len() > MAXIMUM_PENDING_TRANSFERS {
                return Err(format!(
                    "Invalid {} message. \
                                    Adding the transfer would exceed the allowed limit of {} \
                                    pending transfers per channel.",
                    message, MAXIMUM_PENDING_TRANSFERS
                ));
            }
            pending_locks
        }
        None => {
            return Err(format!("Invalid {} message. Same lock handled twice", message));
        }
    };

    let locksroot_with_lock = compute_locksroot(&pending_locks);
    if received_balance_proof.locksroot != locksroot_with_lock {
        return Err(format!(
            "Invalid {} message. Balance proof's lock didn't match. \
                            expected: {:?} \
                            got: {:?}",
            message, locksroot_with_lock, received_balance_proof.locksroot
        ));
    } else if received_balance_proof.transferred_amount != current_transferred_amount {
        return Err(format!(
            "Invalid {} message. Balance proof's transferred_amount changed. \
                            expected: {} \
                            got: {}",
            message, current_transferred_amount, received_balance_proof.transferred_amount
        ));
    } else if received_balance_proof.locked_amount != expected_locked_amount {
        return Err(format!(
            "Invalid {} message. Balance proof's locked_amount changed. \
                            expected: {} \
                            got: {}",
            message, expected_locked_amount, received_balance_proof.locked_amount
        ));
    } else if lock.amount > distributable {
        return Err(format!(
            "Invalid {} message. Lock amount larger than the available distributable. \
                            Lock amount: {}, maximum distributable: {}",
            message, lock.amount, distributable
        ));
    }

    Ok(pending_locks)
}

fn is_valid_locked_transfer(
    transfer_state: &LockedTransferState,
    channel_state: &ChannelState,
    sender_end_state: &mut ChannelEndState,
    receiver_end_state: &ChannelEndState,
) -> Result<PendingLocksState, String> {
    valid_locked_transfer_check(
        channel_state,
        sender_end_state,
        receiver_end_state,
        "LockedTransfer",
        &transfer_state.balance_proof,
        &transfer_state.lock,
    )
}

pub(super) fn handle_receive_lock_expired(
    channel_state: &mut ChannelState,
    state_change: ReceiveLockExpired,
    block_number: BlockNumber,
    recipient_metadata: Option<AddressMetadata>,
) -> TransitionResult {
    let sender = match state_change.balance_proof.sender {
        Some(sender) => sender,
        None => {
            return Err(StateTransitionError {
                msg: "The transfer's sender is None".to_owned(),
            });
        }
    };
    let validate_pending_locks = is_valid_lock_expired(
        channel_state,
        state_change.clone(),
        &channel_state.partner_state,
        &channel_state.our_state,
        block_number,
    );

    let events = match validate_pending_locks {
        Ok(pending_locks) => {
            let nonce = state_change.balance_proof.nonce;
            channel_state.partner_state.balance_proof = Some(state_change.balance_proof);
            channel_state.partner_state.nonce = nonce;
            channel_state.partner_state.pending_locks = pending_locks;

            delete_unclaimed_lock(&mut channel_state.partner_state, state_change.secrethash);

            let send_processed = SendProcessed {
                inner: SendMessageEventInner {
                    recipient: sender,
                    recipient_metadata,
                    canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
                    message_identifier: state_change.message_identifier,
                },
            };
            vec![send_processed.into()]
        }
        Err(e) => {
            let invalid_lock_expired = ErrorInvalidReceivedLockExpired {
                secrethash: state_change.secrethash,
                reason: e,
            };
            vec![invalid_lock_expired.into()]
        }
    };

    Ok(ChannelTransition {
        new_state: Some(channel_state.clone()),
        events,
    })
}

pub(super) fn handle_receive_locked_transfer(
    channel_state: &mut ChannelState,
    mediated_transfer: LockedTransferState,
    recipient_metadata: Option<AddressMetadata>,
) -> Result<Event, String> {
    let sender = mediated_transfer
        .balance_proof
        .sender
        .ok_or("The transfer's sender is None")?;

    match is_valid_locked_transfer(
        &mediated_transfer,
        &channel_state.clone(),
        &mut channel_state.partner_state,
        &channel_state.our_state,
    ) {
        Ok(pending_locks) => {
            channel_state.partner_state.balance_proof = Some(mediated_transfer.balance_proof.clone());
            channel_state.partner_state.nonce = mediated_transfer.balance_proof.nonce;
            channel_state.partner_state.pending_locks = pending_locks;

            let lock = mediated_transfer.lock;
            channel_state
                .partner_state
                .secrethashes_to_lockedlocks
                .insert(lock.secrethash, lock);

            Ok(SendProcessed {
                inner: SendMessageEventInner {
                    recipient: sender,
                    recipient_metadata,
                    canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
                    message_identifier: mediated_transfer.message_identifier,
                },
            }
            .into())
        }
        Err(e) => Ok(ErrorInvalidReceivedLockedTransfer {
            payment_identifier: mediated_transfer.payment_identifier,
            reason: e,
        }
        .into()),
    }
}

pub(super) fn handle_refund_transfer(
    channel_state: &mut ChannelState,
    received_transfer: LockedTransferState,
    refund: ReceiveTransferRefund,
) -> Result<Event, String> {
    let pending_locks = is_valid_refund(
        &channel_state.clone(),
        refund.clone(),
        &mut channel_state.partner_state,
        &channel_state.our_state,
        &received_transfer,
    );
    let event = match pending_locks {
        Ok(pending_locks) => {
            channel_state.partner_state.balance_proof = Some(refund.transfer.balance_proof.clone());
            channel_state.partner_state.nonce = refund.transfer.balance_proof.nonce;
            channel_state.partner_state.pending_locks = pending_locks;

            let lock = refund.transfer.lock;
            channel_state
                .partner_state
                .secrethashes_to_lockedlocks
                .insert(lock.secrethash, lock);

            let recipient_address = channel_state.partner_state.address;
            let recipient_metadata = get_address_metadata(recipient_address, received_transfer.route_states.clone());
            SendProcessed {
                inner: SendMessageEventInner {
                    recipient: recipient_address,
                    recipient_metadata,
                    canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
                    message_identifier: refund.transfer.message_identifier,
                },
            }
            .into()
        }
        Err(msg) => ErrorInvalidReceivedTransferRefund {
            payment_identifier: received_transfer.payment_identifier,
            reason: msg,
        }
        .into(),
    };
    Ok(event)
}

fn handle_block(
    mut channel_state: ChannelState,
    state_change: Block,
    block_number: BlockNumber,
    pseudo_random_number_generator: &mut Random,
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

        let settlement_end = channel_state.settle_timeout.saturating_add(*closed_block_number).into();
        let state_change_block_number: BlockNumber = state_change.block_number;
        if state_change_block_number > settlement_end {
            channel_state.settle_transaction = Some(TransactionExecutionStatus {
                started_block_number: Some(state_change.block_number),
                finished_block_number: None,
                result: None,
            });

            events.push(
                ContractSendChannelSettle {
                    inner: ContractSendEventInner {
                        triggered_by_blockhash: state_change.block_hash,
                    },
                    canonical_identifier: channel_state.canonical_identifier.clone(),
                }
                .into(),
            );
        }
    }

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn set_closed(mut channel_state: ChannelState, block_number: BlockNumber) -> ChannelState {
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
            let expiration = channel_state
                .settle_timeout
                .saturating_add(*state_change.block_number)
                .into();
            let update = ContractSendChannelUpdateTransfer {
                inner: ContractSendEventInner {
                    triggered_by_blockhash: state_change.block_hash,
                },
                balance_proof,
                expiration,
            };
            channel_state.update_transaction = Some(TransactionExecutionStatus {
                started_block_number: Some(state_change.block_number),
                finished_block_number: None,
                result: None,
            });
            events.push(update.into());
        }
    }

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn set_settled(mut channel_state: ChannelState, block_number: BlockNumber) -> ChannelState {
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
    mut channel_state: ChannelState,
    state_change: ContractReceiveChannelSettled,
) -> TransitionResult {
    let mut events = vec![];

    if state_change.canonical_identifier == channel_state.canonical_identifier {
        channel_state = set_settled(channel_state.clone(), state_change.block_number);
        let our_locksroot = state_change.our_onchain_locksroot.clone();
        let partner_locksroot = state_change.our_onchain_locksroot.clone();
        let should_clear_channel =
            our_locksroot == Locksroot::from(vec![]) && partner_locksroot == Locksroot::from(vec![]);

        if should_clear_channel {
            return Ok(ChannelTransition {
                new_state: None,
                events,
            });
        }

        channel_state.our_state.onchain_locksroot = our_locksroot;
        channel_state.partner_state.onchain_locksroot = partner_locksroot;

        events.push(
            ContractSendChannelBatchUnlock {
                inner: ContractSendEventInner {
                    triggered_by_blockhash: state_change.block_hash,
                },
                canonical_identifier: channel_state.canonical_identifier.clone(),
                sender: channel_state.partner_state.address,
            }
            .into(),
        );
    }

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn update_contract_balance(end_state: &mut ChannelEndState, contract_balance: TokenAmount) {
    if contract_balance > end_state.contract_balance {
        end_state.contract_balance = contract_balance;
    }
}

/// Returns a list of numbers from start to stop (inclusive).
#[allow(dead_code)]
fn linspace(start: TokenAmount, stop: TokenAmount, num: TokenAmount) -> Vec<TokenAmount> {
    // assert num > 1, "Must generate at least one step"
    // assert start <= stop, "start must be smaller than stop"

    let step = (stop - start) / (num - 1);

    let mut result = vec![];

    let mut i = TokenAmount::zero();
    while i < num {
        result.push(start + i * step);
        i = i + 1;
    }

    result
}

#[allow(dead_code)]
fn calculate_imbalance_fees(
    channel_capacity: TokenAmount,
    proportional_imbalance_fee: TokenAmount,
) -> Option<Vec<(TokenAmount, FeeAmount)>> {
    if proportional_imbalance_fee == TokenAmount::zero() {
        return None;
    }

    if channel_capacity == TokenAmount::zero() {
        return None;
    }

    let maximum_slope = TokenAmount::from(10); // 0.1
    let (max_imbalance_fee, overflow) = channel_capacity.overflowing_mul(proportional_imbalance_fee);

    if overflow {
        // TODO: Should fail?
        return None;
    }

    let max_imbalance_fee = max_imbalance_fee / TokenAmount::from(1_000_000);
    // assert proportional_imbalance_fee / 1e6 <= maximum_slope / 2, "Too high imbalance fee"

    // calculate function parameters
    let s = maximum_slope;
    let c = max_imbalance_fee;
    let o = channel_capacity.div(2);
    let b = o.div(s).div(c);
    let b = b.min(TokenAmount::from(10)); // limit exponent to keep numerical stability;
    let a = c / o.pow(b);

    let f = |x: TokenAmount| -> TokenAmount { a * (x - o).pow(b) };

    // calculate discrete function points
    let num_base_points = min(NUM_DISCRETISATION_POINTS.into(), channel_capacity + 1);
    let x_values: Vec<TokenAmount> = linspace(0u64.into(), channel_capacity, num_base_points);
    let y_values: Vec<TokenAmount> = x_values.iter().map(|x| f(*x)).collect();

    Some(x_values.into_iter().zip(y_values).collect())
}

#[allow(dead_code)]
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

    //update_fee_schedule_after_balance_change(&mut channel_state, state_change.fee_config);

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

    // update_fee_schedule_after_balance_change(&mut channel_state, state_change.fee_config);

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
            channel_state.our_state.onchain_locksroot = Locksroot::from(vec![]);
        } else if state_change.sender == channel_state.partner_state.address {
            channel_state.partner_state.onchain_locksroot = Locksroot::from(vec![]);
        }

        let no_unlocks_left_to_do = channel_state.our_state.onchain_locksroot == Locksroot::from(vec![])
            && channel_state.partner_state.onchain_locksroot == Locksroot::from(vec![]);
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
    block_number: BlockNumber,
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

fn is_valid_total_withdraw(
    channel_state: &ChannelState,
    our_total_withdraw: TokenAmount,
    allow_zero: bool,
) -> Result<(), String> {
    let balance = views::get_channel_balance(&channel_state.our_state, &channel_state.partner_state);

    if our_total_withdraw
        .checked_add(channel_state.partner_total_withdraw())
        .is_none()
    {
        return Err(format!(
            "The new total_withdraw {} will cause an overflow",
            our_total_withdraw
        ));
    }

    let withdraw_amount = our_total_withdraw - channel_state.our_total_withdraw();

    if channel_state.status() != ChannelStatus::Opened {
        return Err(format!("Invalid withdraw, the channel is not opened"));
    } else if withdraw_amount < TokenAmount::zero() {
        return Err(format!("Total withdraw {} decreased", our_total_withdraw));
    } else if !allow_zero && withdraw_amount == TokenAmount::zero() {
        return Err(format!("Total withdraw {} did not increase", our_total_withdraw));
    } else if balance < withdraw_amount {
        return Err(format!(
            "Insufficient balance: {}. Request {} for withdraw",
            balance, our_total_withdraw
        ));
    }

    return Ok(());
}

fn is_valid_withdraw_signature(
    canonical_identifier: CanonicalIdentifier,
    sender: Address,
    participant: Address,
    total_withdraw: TokenAmount,
    expiration_block: BlockExpiration,
    withdraw_signature: Signature,
) -> Result<(), String> {
    let packed = pack_withdraw(canonical_identifier, participant, total_withdraw, expiration_block);
    is_valid_signature(packed, withdraw_signature, sender)
}

/// Determine whether a withdraw has expired.
///
/// The withdraw has expired if the current block exceeds
/// the withdraw's expiration + confirmation blocks.
fn is_withdraw_expired(block_number: BlockNumber, expiration_threshold: BlockExpiration) -> bool {
    block_number >= expiration_threshold
}

fn is_valid_withdraw_expired(
    channel_state: &ChannelState,
    state_change: &ReceiveWithdrawExpired,
    withdraw_state: &PendingWithdrawState,
    block_number: BlockNumber,
) -> Result<(), String> {
    let expected_nonce = get_next_nonce(&channel_state.partner_state);
    let is_withdraw_expired = is_withdraw_expired(
        block_number,
        get_receiver_expiration_threshold(withdraw_state.expiration),
    );

    if !is_withdraw_expired {
        return Err(format!(
            "WithdrawExpired for withdraw that has not yet expired {}",
            state_change.total_withdraw
        ));
    } else if channel_state.canonical_identifier != state_change.canonical_identifier {
        return Err(format!("Invalid canonical identifier provided in withdraw request"));
    } else if state_change.sender != channel_state.partner_state.address {
        return Err(format!("Invalid sender. Request must be sent by the partner"));
    } else if state_change.total_withdraw != withdraw_state.total_withdraw {
        return Err(format!(
            "WithdrawExpired for local withdraw amounts do not match. \
                            Received {}, local amount {}",
            state_change.total_withdraw, withdraw_state.total_withdraw
        ));
    } else if state_change.nonce != expected_nonce {
        return Err(format!(
            "Nonce did not change sequentially. Expected: {}, got {}",
            expected_nonce, state_change.nonce
        ));
    }

    Ok(())
}

fn is_valid_withdraw_request(
    channel_state: &ChannelState,
    withdraw_request: &ReceiveWithdrawRequest,
) -> Result<(), String> {
    let expected_nonce = get_next_nonce(&channel_state.partner_state);
    let balance = views::get_channel_balance(&channel_state.partner_state, &channel_state.our_state);

    let is_valid = is_valid_withdraw_signature(
        withdraw_request.canonical_identifier.clone(),
        withdraw_request.sender,
        withdraw_request.participant,
        withdraw_request.total_withdraw,
        withdraw_request.expiration,
        withdraw_request.signature,
    );

    let withdraw_amount = withdraw_request.total_withdraw - channel_state.partner_total_withdraw();

    if withdraw_request
        .total_withdraw
        .checked_add(channel_state.our_total_withdraw())
        .is_none()
    {
        return Err(format!(
            "The new total_withdraw {} will cause an overflow",
            withdraw_request.total_withdraw
        ));
    }

    // The confirming node must accept an expired withdraw request. This is
    // necessary to clear the requesting node's queue. This is not a security
    // flaw because the smart contract will not allow the withdraw to happen.
    if channel_state.canonical_identifier != withdraw_request.canonical_identifier {
        return Err(format!("Invalid canonical identifier provided in withdraw request"));
    } else if withdraw_request.participant != channel_state.partner_state.address {
        return Err(format!("Invalid participant. It must be the partner's address"));
    } else if withdraw_request.sender != channel_state.partner_state.address {
        return Err(format!("Invalid sender. Request must be sent by the partner"));
    } else if withdraw_amount < TokenAmount::zero() {
        return Err(format!("Total withdraw {} decreased", withdraw_request.total_withdraw));
    } else if balance < withdraw_amount {
        return Err(format!(
            "Insufficient balance: {}. Request {} for withdraw",
            balance, withdraw_amount
        ));
    } else if withdraw_request.nonce != expected_nonce {
        return Err(format!(
            "Nonce did not change sequentially. Expected: {}, got {}",
            expected_nonce, withdraw_request.nonce
        ));
    }

    is_valid
}

fn is_valid_withdraw_confirmation(
    channel_state: &ChannelState,
    received_withdraw: &ReceiveWithdrawConfirmation,
) -> Result<(), String> {
    let expiration = match channel_state
        .our_state
        .withdraws_pending
        .get(&received_withdraw.total_withdraw)
    {
        Some(withdraw_state) => Some(withdraw_state.expiration),
        None => {
            if channel_state.our_state.withdraws_expired.len() > 0 {
                let expiration = channel_state
                    .our_state
                    .withdraws_expired
                    .iter()
                    .filter(|candidate| candidate.total_withdraw == received_withdraw.total_withdraw)
                    .next()
                    .map(|state| state.expiration);
                expiration
            } else {
                None
            }
        }
    };

    let expiration = match expiration {
        Some(expiration) => expiration,
        None => {
            return Err(format!(
                "Received withdraw confirmation {} was not found in withdraw states",
                received_withdraw.total_withdraw
            ));
        }
    };

    let expected_nonce = get_next_nonce(&channel_state.partner_state);

    if received_withdraw
        .total_withdraw
        .checked_add(channel_state.our_total_withdraw())
        .is_none()
    {
        return Err(format!(
            "The new total_withdraw {} will cause an overflow",
            received_withdraw.total_withdraw
        ));
    } else if channel_state.canonical_identifier != received_withdraw.canonical_identifier {
        return Err(format!("Invalid canonical identifier provided in withdraw request"));
    } else if received_withdraw.participant != channel_state.our_state.address {
        return Err(format!("Invalid participant. It must be our address"));
    } else if received_withdraw.sender != channel_state.partner_state.address {
        return Err(format!("Invalid sender. Request must be sent by the partner"));
    } else if received_withdraw.total_withdraw != channel_state.our_total_withdraw() {
        return Err(format!(
            "Total withdraw confirmation {} does not match our total withdraw {}",
            received_withdraw.total_withdraw,
            channel_state.our_total_withdraw()
        ));
    } else if received_withdraw.nonce != expected_nonce {
        return Err(format!(
            "Nonce did not change sequentially. Expected: {}, got {}",
            expected_nonce, received_withdraw.nonce
        ));
    } else if expiration != received_withdraw.expiration {
        return Err(format!(
            "Invalid expiration {}, withdraw confirmation \
                            must use the same confirmation as the request \
                            otherwise the signature will not match on-chain",
            received_withdraw.expiration
        ));
    }

    is_valid_withdraw_signature(
        received_withdraw.canonical_identifier.clone(),
        received_withdraw.sender,
        received_withdraw.participant,
        received_withdraw.total_withdraw,
        received_withdraw.expiration,
        received_withdraw.signature,
    )
}

fn is_valid_action_coop_settle(channel_state: &ChannelState, total_withdraw: TokenAmount) -> Result<(), String> {
    is_valid_total_withdraw(channel_state, total_withdraw, true)?;

    if channel_state.our_state.pending_locks.locks.len() > 0 {
        return Err(format!("Coop-Settle not allowed: we still have pending locks"));
    }
    if channel_state.partner_state.pending_locks.locks.len() > 0 {
        return Err(format!("Coop-Settle not allowed: partner still has pending locks"));
    }
    if channel_state.our_state.offchain_total_withdraw() > TokenAmount::zero() {
        return Err(format!("Coop-Settle not allowed: We still have pending withdraws"));
    }
    if channel_state.partner_state.offchain_total_withdraw() > TokenAmount::zero() {
        return Err(format!("Coop-Settle not allowed: partner still has pending withdraws"));
    }

    Ok(())
}

fn is_valid_unlock(
    channel_state: &ChannelState,
    sender_state: &mut ChannelEndState,
    unlock: ReceiveUnlock,
) -> Result<PendingLocksState, String> {
    let received_balance_proof = unlock.balance_proof;
    let (_, _, current_transferred_amount, current_locked_amount) = get_current_balance_proof(sender_state);
    let lock = match get_lock(sender_state, unlock.secrethash) {
        Some(lock) => lock,
        None => {
            return Err(format!(
                "Invalid unlock message. There is no corresponding lock for {}",
                unlock.secrethash
            ));
        }
    };

    let pending_locks = match compute_locks_without(&mut sender_state.pending_locks, &lock) {
        Some(pending_locks) => pending_locks,
        None => {
            return Err(format!(
                "Invalid unlock message. The lock is unknown {}",
                unlock.secrethash
            ));
        }
    };

    let locksroot_without_lock = compute_locksroot(&pending_locks);

    let expected_transferred_amount = current_transferred_amount + lock.amount;
    let expected_locked_amount = current_locked_amount - lock.amount;

    let is_valid_balance_proof = is_balance_proof_usable_onchain(&received_balance_proof, channel_state, sender_state);

    if let Err(e) = is_valid_balance_proof {
        return Err(format!("Invalid unlock message. {}", e));
    } else if received_balance_proof.locksroot != locksroot_without_lock {
        // Unlock messages remove a known lock, the new locksroot must have only
        // that lock removed, otherwise the sender may be trying to remove
        // additional locks.
        return Err(format!(
            "Invalid unlock message. \
                            Balance proof's locksroot didn't match. \
                            expected {:?} \
                            got {:?}",
            locksroot_without_lock, received_balance_proof.locksroot
        ));
    } else if received_balance_proof.transferred_amount != expected_transferred_amount {
        return Err(format!(
            "Invalid unlock message. \
                            Balance proof's wrong transferred_amount. \
                            expected {} \
                            got {}",
            expected_transferred_amount, received_balance_proof.transferred_amount
        ));
    } else if received_balance_proof.locked_amount != expected_locked_amount {
        return Err(format!(
            "Invalid unlock message. \
                            Balance proof's wrong locked_amount. \
                            expected {} \
                            got {}",
            expected_locked_amount, received_balance_proof.locked_amount
        ));
    }

    Ok(pending_locks)
}

fn is_valid_refund(
    channel_state: &ChannelState,
    refund: ReceiveTransferRefund,
    sender_state: &mut ChannelEndState,
    receiver_state: &ChannelEndState,
    received_transfer: &LockedTransferState,
) -> Result<PendingLocksState, String> {
    let pending_locks = valid_locked_transfer_check(
        channel_state,
        sender_state,
        receiver_state,
        "RefundTransfer",
        &refund.transfer.balance_proof,
        &refund.transfer.lock,
    )?;

    if !refund_transfer_matches_transfer(&refund.transfer, received_transfer) {
        return Err("Refund transfer did not match the received transfer".to_owned());
    }

    Ok(pending_locks)
}

fn is_valid_action_withdraw(channel_state: &ChannelState, withdraw: &ActionChannelWithdraw) -> Result<(), String> {
    let balance = views::get_channel_balance(&channel_state.our_state, &channel_state.partner_state);
    let (_, overflow) = withdraw
        .total_withdraw
        .overflowing_add(channel_state.partner_state.total_withdraw());

    let withdraw_amount = withdraw.total_withdraw - channel_state.our_state.total_withdraw();

    if channel_state.status() != ChannelStatus::Opened {
        return Err("Invalid withdraw, the channel is not opened".to_owned());
    } else if withdraw_amount == TokenAmount::zero() {
        return Err(format!("Total withdraw {:?} did not increase", withdraw.total_withdraw));
    } else if balance < withdraw_amount {
        return Err(format!(
            "Insufficient balance: {:?}. Requested {:?} for withdraw",
            balance, withdraw_amount
        ));
    } else if overflow {
        return Err(format!(
            "The new total_withdraw {:?} will cause an overflow",
            withdraw.total_withdraw
        ));
    }

    return Ok(());
}

/// This will register the secret and set the lock to the unlocked stated.
///
/// Even though the lock is unlock it is *not* claimed. The capacity will
/// increase once the next balance proof is received.
fn register_secret_endstate(end_state: &mut ChannelEndState, secret: Secret, secrethash: SecretHash) {
    if is_lock_locked(end_state, secrethash) {
        let pending_lock = match end_state.secrethashes_to_lockedlocks.get(&secrethash) {
            Some(lock) => lock.clone(),
            None => return,
        };

        end_state.secrethashes_to_lockedlocks.remove(&secrethash);

        end_state.secrethashes_to_unlockedlocks.insert(
            secrethash,
            UnlockPartialProofState {
                lock: pending_lock.clone(),
                secret,
                amount: pending_lock.amount,
                expiration: pending_lock.expiration,
                secrethash,
                encoded: pending_lock.encoded,
            },
        );
    }
}

pub(super) fn register_offchain_secret(channel_state: &mut ChannelState, secret: Secret, secrethash: SecretHash) {
    register_secret_endstate(&mut channel_state.our_state, secret.clone(), secrethash);
    register_secret_endstate(&mut channel_state.partner_state, secret, secrethash);
}

fn send_withdraw_request(
    channel_state: &mut ChannelState,
    total_withdraw: TokenAmount,
    block_number: BlockNumber,
    pseudo_random_number_generator: &mut Random,
    recipient_metadata: Option<AddressMetadata>,
    coop_settle: bool,
) -> Vec<Event> {
    let good_channel = CHANNEL_STATES_PRIOR_TO_CLOSE
        .to_vec()
        .iter()
        .position(|status| status == &channel_state.status())
        .is_some();

    if !good_channel {
        return vec![];
    }

    let nonce = channel_state.our_state.next_nonce();
    let expiration = get_safe_initial_expiration(block_number, channel_state.reveal_timeout, None);

    let withdraw_state = PendingWithdrawState {
        total_withdraw,
        expiration,
        nonce,
        recipient_metadata,
    };

    channel_state.our_state.nonce = nonce;
    channel_state
        .our_state
        .withdraws_pending
        .insert(withdraw_state.total_withdraw, withdraw_state.clone());

    vec![SendWithdrawRequest {
        inner: SendMessageEventInner {
            recipient: channel_state.partner_state.address,
            recipient_metadata: withdraw_state.recipient_metadata.clone(),
            canonical_identifier: channel_state.canonical_identifier.clone(),
            message_identifier: pseudo_random_number_generator.next(),
        },
        participant: channel_state.our_state.address,
        nonce: channel_state.our_state.nonce,
        expiration: withdraw_state.expiration,
        coop_settle,
    }
    .into()]
}

fn events_for_close(
    channel_state: &mut ChannelState,
    block_number: BlockNumber,
    block_hash: BlockHash,
) -> Result<Vec<Event>, String> {
    if !CHANNEL_STATES_PRIOR_TO_CLOSE.contains(&channel_state.status()) {
        return Ok(vec![]);
    }

    channel_state.close_transaction = Some(TransactionExecutionStatus {
        started_block_number: Some(block_number),
        finished_block_number: None,
        result: None,
    });

    let balance_proof = match &channel_state.partner_state.balance_proof {
        Some(balance_proof) => {
            if balance_proof.signature.is_none() {
                return Err("Balance proof is not signed".to_owned().into());
            }
            balance_proof
        }
        None => {
            return Err("No Balance proof found".to_owned().into());
        }
    };

    let close_event = ContractSendChannelClose {
        inner: ContractSendEventInner {
            triggered_by_blockhash: block_hash,
        },
        canonical_identifier: channel_state.canonical_identifier.clone(),
        balance_proof: balance_proof.clone(),
    };

    Ok(vec![close_event.into()])
}

fn events_for_coop_settle(
    channel_state: &ChannelState,
    coop_settle_state: &mut CoopSettleState,
    block_number: BlockNumber,
    block_hash: BlockHash,
) -> Vec<Event> {
    if let Some(partner_signature_request) = coop_settle_state.partner_signature_request {
        if let Some(partner_signature_confirmation) = coop_settle_state.partner_signature_confirmation {
            if coop_settle_state.expiration >= block_number - channel_state.reveal_timeout {
                let send_coop_settle = ContractSendChannelCoopSettle {
                    inner: ContractSendEventInner {
                        triggered_by_blockhash: block_hash,
                    },
                    canonical_identifier: channel_state.canonical_identifier.clone(),
                    our_total_withdraw: coop_settle_state.total_withdraw_initiator,
                    partner_total_withdraw: coop_settle_state.total_withdraw_partner,
                    expiration: coop_settle_state.expiration,
                    signature_our_withdraw: partner_signature_confirmation,
                    signature_partner_withdraw: partner_signature_request,
                };

                coop_settle_state.transaction = Some(TransactionExecutionStatus {
                    started_block_number: Some(block_number),
                    finished_block_number: None,
                    result: None,
                });

                return vec![send_coop_settle.into()];
            }
        }
    }
    vec![]
}

fn handle_action_close(
    mut channel_state: ChannelState,
    state_change: ActionChannelClose,
    block_number: BlockNumber,
    block_hash: BlockHash,
) -> TransitionResult {
    if channel_state.canonical_identifier != state_change.canonical_identifier {
        return Err("Caller must ensure the canonical IDs match".to_owned().into());
    }

    let events = events_for_close(&mut channel_state, block_number, block_hash).map_err(Into::into)?;

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn handle_action_withdraw(
    mut channel_state: ChannelState,
    state_change: ActionChannelWithdraw,
    block_number: BlockNumber,
    pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
    let mut events = vec![];
    match is_valid_action_withdraw(&channel_state, &state_change) {
        Ok(_) => {
            events = send_withdraw_request(
                &mut channel_state,
                state_change.total_withdraw,
                block_number,
                pseudo_random_number_generator,
                state_change.recipient_metadata,
                false,
            );
        }
        Err(e) => {
            events.push(
                ErrorInvalidActionWithdraw {
                    attemped_withdraw: state_change.total_withdraw,
                    reason: e,
                }
                .into(),
            );
        }
    };
    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn handle_action_set_channel_reveal_timeout(
    mut channel_state: ChannelState,
    state_change: ActionChannelSetRevealTimeout,
) -> TransitionResult {
    let double_reveal_timeout: BlockNumber = state_change.reveal_timeout.mul(2u64).into();
    let is_valid_reveal_timeout =
        state_change.reveal_timeout >= 7u64.into() && channel_state.settle_timeout >= double_reveal_timeout;
    if !is_valid_reveal_timeout {
        return Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![ErrorInvalidActionSetRevealTimeout {
                reveal_timeout: state_change.reveal_timeout,
                reason: format!("Settle timeout should be at least twice as large as reveal timeout"),
            }
            .into()],
        });
    }

    channel_state.reveal_timeout = state_change.reveal_timeout;
    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events: vec![],
    })
}

fn handle_action_coop_settle(
    mut channel_state: ChannelState,
    state_change: ActionChannelCoopSettle,
    block_number: BlockNumber,
    pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
    let our_max_total_withdraw = get_max_withdraw_amount(&channel_state.our_state, &channel_state.partner_state);
    let partner_max_total_withdraw = get_max_withdraw_amount(&channel_state.partner_state, &channel_state.our_state);

    let mut events = vec![];
    match is_valid_action_coop_settle(&channel_state, our_max_total_withdraw) {
        Ok(_) => {
            let expiration = get_safe_initial_expiration(block_number, channel_state.reveal_timeout, None);
            let coop_settle = CoopSettleState {
                total_withdraw_initiator: our_max_total_withdraw,
                total_withdraw_partner: partner_max_total_withdraw,
                expiration,
                partner_signature_request: None,
                partner_signature_confirmation: None,
                transaction: None,
            };

            channel_state.our_state.initiated_coop_settle = Some(coop_settle);

            let withdraw_request_events = send_withdraw_request(
                &mut channel_state,
                our_max_total_withdraw,
                block_number,
                pseudo_random_number_generator,
                state_change.recipient_metadata,
                false,
            );
            events.extend(withdraw_request_events);
        }
        Err(e) => events.push(
            ErrorInvalidActionCoopSettle {
                attempted_withdraw: our_max_total_withdraw,
                reason: e,
            }
            .into(),
        ),
    };

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn handle_receive_withdraw_request(
    mut channel_state: ChannelState,
    state_change: ReceiveWithdrawRequest,
    block_number: BlockNumber,
    block_hash: BlockHash,
    pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
    let mut events = vec![];
    if let Err(msg) = is_valid_withdraw_request(&channel_state, &state_change) {
        return Ok(ChannelTransition {
            new_state: Some(channel_state),
            events: vec![ErrorInvalidReceivedWithdrawRequest {
                attemped_withdraw: state_change.total_withdraw,
                reason: msg,
            }
            .into()],
        });
    }

    let withdraw_state = PendingWithdrawState {
        total_withdraw: state_change.total_withdraw,
        expiration: state_change.expiration,
        nonce: state_change.nonce,
        recipient_metadata: state_change.sender_metadata.clone(),
    };
    channel_state
        .partner_state
        .withdraws_pending
        .insert(withdraw_state.total_withdraw, withdraw_state);
    channel_state.partner_state.nonce = state_change.nonce;

    if channel_state.our_state.initiated_coop_settle.is_none() || state_change.coop_settle {
        let partner_max_total_withdraw =
            get_max_withdraw_amount(&channel_state.partner_state, &channel_state.our_state);
        if partner_max_total_withdraw != state_change.total_withdraw {
            return Ok(ChannelTransition {
                new_state: Some(channel_state),
                events: vec![ErrorInvalidReceivedWithdrawRequest {
                    attemped_withdraw: state_change.total_withdraw,
                    reason: format!(
                        "Partner did not withdraw with maximum balance. Should be {}",
                        partner_max_total_withdraw
                    ),
                }
                .into()],
            });
        }

        if channel_state.partner_state.pending_locks.locks.len() > 0 {
            return Ok(ChannelTransition {
                new_state: Some(channel_state),
                events: vec![ErrorInvalidReceivedWithdrawRequest {
                    attemped_withdraw: state_change.total_withdraw,
                    reason: format!("Partner has pending transfers"),
                }
                .into()],
            });
        }

        if let Some(our_initiated_coop_settle) = channel_state.our_state.initiated_coop_settle.clone().as_mut() {
            // There is a coop-settle inplace that we initiated
            // and partner is communicating their total withdraw with us
            if our_initiated_coop_settle.expiration != state_change.expiration {
                return Ok(ChannelTransition {
                    new_state: Some(channel_state),
                    events: vec![ErrorInvalidReceivedWithdrawRequest {
                        attemped_withdraw: state_change.total_withdraw,
                        reason: format!(
                            "Partner requested withdraw while we initiated a coop-settle: \
                                             Partner's withdraw has differing expiration."
                        ),
                    }
                    .into()],
                });
            }

            if our_initiated_coop_settle.total_withdraw_partner != state_change.total_withdraw {
                return Ok(ChannelTransition {
                    new_state: Some(channel_state),
                    events: vec![ErrorInvalidReceivedWithdrawRequest {
                        attemped_withdraw: state_change.total_withdraw,
                        reason: format!(
                            "The expected total withdraw of the partner does not match the withdraw request"
                        ),
                    }
                    .into()],
                });
            }

            our_initiated_coop_settle.partner_signature_request = Some(state_change.signature);
            let coop_settle_events =
                events_for_coop_settle(&channel_state, our_initiated_coop_settle, block_number, block_hash);
            channel_state.our_state.initiated_coop_settle = Some(our_initiated_coop_settle.clone());
            events.extend(coop_settle_events);
        } else {
            let our_max_total_withdraw =
                get_max_withdraw_amount(&channel_state.our_state, &channel_state.partner_state);

            if channel_state.our_state.pending_locks.locks.len() > 0 {
                return Ok(ChannelTransition {
                    new_state: Some(channel_state),
                    events: vec![ErrorInvalidReceivedWithdrawRequest {
                        attemped_withdraw: state_change.total_withdraw,
                        reason: format!("Partner initiated coop-settle but we have pending transfers"),
                    }
                    .into()],
                });
            }

            let partner_initiated_coop_settle = CoopSettleState {
                total_withdraw_initiator: state_change.total_withdraw,
                total_withdraw_partner: our_max_total_withdraw,
                expiration: state_change.expiration,
                partner_signature_request: Some(state_change.signature),
                partner_signature_confirmation: None,
                transaction: None,
            };
            channel_state.partner_state.initiated_coop_settle = Some(partner_initiated_coop_settle);
            let send_withdraw_request_events = send_withdraw_request(
                &mut channel_state,
                our_max_total_withdraw,
                state_change.expiration,
                pseudo_random_number_generator,
                state_change.sender_metadata.clone(),
                false,
            );
            events.extend(send_withdraw_request_events);
        }
    }

    channel_state.our_state.nonce = get_next_nonce(&channel_state.our_state);
    let send_withdraw = SendWithdrawConfirmation {
        inner: SendMessageEventInner {
            recipient: channel_state.partner_state.address,
            recipient_metadata: state_change.sender_metadata,
            canonical_identifier: channel_state.canonical_identifier.clone(),
            message_identifier: state_change.message_identifier,
        },
        participant: channel_state.partner_state.address,
        total_withdraw: state_change.total_withdraw,
        nonce: channel_state.our_state.nonce,
        expiration: state_change.expiration,
    };
    events.push(send_withdraw.into());

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn handle_receive_withdraw_confirmation(
    mut channel_state: ChannelState,
    state_change: ReceiveWithdrawConfirmation,
    block_number: BlockNumber,
    block_hash: BlockHash,
) -> TransitionResult {
    let is_valid = is_valid_withdraw_confirmation(&channel_state, &state_change);

    let withdraw_state = channel_state
        .our_state
        .withdraws_pending
        .get(&state_change.total_withdraw);
    let mut recipient_metadata = None;
    if let Some(withdraw_state) = withdraw_state {
        recipient_metadata = withdraw_state.recipient_metadata.clone();
    }

    let mut events = vec![];
    match is_valid {
        Ok(_) => {
            channel_state.partner_state.nonce = state_change.nonce;
            events.push(
                SendProcessed {
                    inner: SendMessageEventInner {
                        recipient: channel_state.partner_state.address,
                        recipient_metadata,
                        canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
                        message_identifier: state_change.message_identifier,
                    },
                }
                .into(),
            );

            let partner_initiated_coop_settle = &channel_state.partner_state.initiated_coop_settle;
            if let Some(our_initiated_coop_settle) = channel_state.our_state.initiated_coop_settle.clone().as_mut() {
                if partner_initiated_coop_settle.is_some() {
                    return Err(format!("Only one party can initiate a coop settle").into());
                }

                our_initiated_coop_settle.partner_signature_confirmation = Some(state_change.signature);

                let coop_settle_events =
                    events_for_coop_settle(&channel_state, our_initiated_coop_settle, block_number, block_hash);
                channel_state.our_state.initiated_coop_settle = Some(our_initiated_coop_settle.clone());
                events.extend(coop_settle_events);
            } else {
                // Normal withdraw
                // Only send the transaction on-chain if there is enough time for the
                // withdraw transaction to be mined
                if partner_initiated_coop_settle.is_none() {
                    if state_change.expiration >= block_number - channel_state.reveal_timeout {
                        let withdraw_onchain = ContractSendChannelWithdraw {
                            inner: ContractSendEventInner {
                                triggered_by_blockhash: block_hash,
                            },
                            canonical_identifier: state_change.canonical_identifier.clone(),
                            total_withdraw: state_change.total_withdraw,
                            expiration: state_change.expiration,
                            partner_signature: state_change.signature,
                        };

                        events.push(withdraw_onchain.into());
                    }
                }
            }
        }
        Err(e) => {
            let invalid_withdraw = ErrorInvalidReceivedWithdrawConfirmation {
                attemped_withdraw: state_change.total_withdraw,
                reason: e,
            };

            events.push(invalid_withdraw.into());
        }
    }

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

fn handle_receive_withdraw_expired(
    mut channel_state: ChannelState,
    state_change: ReceiveWithdrawExpired,
    block_number: BlockNumber,
) -> TransitionResult {
    let mut events = vec![];

    let withdraw_state = match channel_state
        .partner_state
        .withdraws_pending
        .get(&state_change.total_withdraw)
    {
        Some(withdraw_state) => withdraw_state.clone(),
        None => {
            return Ok(ChannelTransition {
                new_state: Some(channel_state),
                events: vec![ErrorInvalidReceivedWithdrawExpired {
                    attemped_withdraw: state_change.total_withdraw,
                    reason: format!(
                        "Withdraw expired of {} did not correspond to a previous withdraw request",
                        state_change.total_withdraw
                    ),
                }
                .into()],
            });
        }
    };

    let is_valid = is_valid_withdraw_expired(&channel_state, &state_change, &withdraw_state, block_number);

    match is_valid {
        Ok(_) => {
            channel_state
                .partner_state
                .withdraws_pending
                .remove(&state_change.total_withdraw);
            channel_state.partner_state.nonce = state_change.nonce;

            if let Some(coop_settle) = channel_state.partner_state.initiated_coop_settle.as_ref() {
                if coop_settle.total_withdraw_initiator == withdraw_state.total_withdraw
                    && coop_settle.expiration == withdraw_state.expiration
                {
                    // We declare the partner's initated coop-settle as expired
                    // and remove it from the state.
                    // This will be used in the handling of incoming withdraw messages.
                    channel_state.partner_state.initiated_coop_settle = None
                }
            }

            events.push(
                SendProcessed {
                    inner: SendMessageEventInner {
                        recipient: channel_state.partner_state.address,
                        recipient_metadata: withdraw_state.recipient_metadata.clone(),
                        canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
                        message_identifier: state_change.message_identifier,
                    },
                }
                .into(),
            )
        }
        Err(e) => {
            events.push(
                ErrorInvalidReceivedWithdrawExpired {
                    attemped_withdraw: state_change.total_withdraw,
                    reason: e,
                }
                .into(),
            );
        }
    }

    Ok(ChannelTransition {
        new_state: Some(channel_state),
        events,
    })
}

/// Some of the checks below are tautologies for the current version of the
/// codebase. However they are kept in there to check the constraints if/when
/// the code changes.
fn sanity_check(transition: ChannelTransition) -> TransitionResult {
    let channel_state = match transition.new_state {
        Some(ref channel_state) => channel_state,
        None => return Ok(transition),
    };
    let partner_state = &channel_state.partner_state;
    let our_state = &channel_state.our_state;

    let mut previous = TokenAmount::zero();
    let coop_settle = our_state.initiated_coop_settle.is_some() || partner_state.initiated_coop_settle.is_some();

    for (total_withdraw, withdraw_state) in our_state.withdraws_pending.iter() {
        if !coop_settle {
            if withdraw_state.total_withdraw <= previous {
                return Err(format!("Total withdraws must be ordered").into());
            }

            if total_withdraw != &withdraw_state.total_withdraw {
                return Err(format!("Total withdraw mismatch").into());
            }

            previous = withdraw_state.total_withdraw;
        }
    }

    let our_balance = views::get_channel_balance(&our_state, &partner_state);
    let partner_balance = views::get_channel_balance(&partner_state, &our_state);

    let channel_capacity = channel_state.capacity();
    if our_balance + partner_balance != channel_capacity {
        return Err(format!("The whole deposit of the channel has to be accounted for.").into());
    }

    let our_locked = get_amount_locked(&our_state);
    let partner_locked = get_amount_locked(&partner_state);

    let (our_bp_locksroot, _, _, our_bp_locked_amount) = get_current_balance_proof(&our_state);
    let (partner_bp_locksroot, _, _, partner_bp_locked_amount) = get_current_balance_proof(&partner_state);

    let message = "The sum of the lock's amounts, and the value of the balance proof \
                   locked_amount must be equal, otherwise settle will not reserve the \
                   proper amount of tokens."
        .to_owned();

    if partner_locked != partner_bp_locked_amount {
        return Err(message.clone().into());
    }
    if our_locked != our_bp_locked_amount {
        return Err(message.into());
    }

    let our_distributable = channel_state.get_distributable(&our_state, &partner_state);
    let partner_distributable = channel_state.get_distributable(&partner_state, &our_state);

    // Because of overflow checks, it is possible for the distributable amount
    // to be lower than the available balance, therefore the sanity check has to
    // be lower-than instead of equal-to
    if our_distributable + our_locked > our_balance {
        return Err(format!("Distributable + locked must not exceed balance (own)").into());
    }
    if partner_distributable + partner_locked > partner_balance {
        return Err(format!("Distributable + locked must not exceed balance (partner)").into());
    }

    let our_locksroot = compute_locksroot(&our_state.pending_locks);
    let partner_locksroot = compute_locksroot(&partner_state.pending_locks);

    let message = "The balance proof locks root must match the existing locks. \
                   Otherwise it is not possible to prove on-chain that a given lock was pending."
        .to_owned();
    if our_locksroot != our_bp_locksroot {
        return Err(message.clone().into());
    }
    if partner_locksroot != partner_bp_locksroot {
        return Err(message.into());
    }

    let message = "The lock mappings and the pending locks must be synchronised. Otherwise there is a bug.".to_owned();
    for lock in partner_state.secrethashes_to_lockedlocks.values() {
        if !partner_state.pending_locks.locks.contains(&lock.encoded) {
            return Err(message.clone().into());
        }
    }
    for lock in partner_state.secrethashes_to_unlockedlocks.values() {
        if !partner_state.pending_locks.locks.contains(&lock.encoded) {
            return Err(message.clone().into());
        }
    }
    for lock in partner_state.secrethashes_to_onchain_unlockedlocks.values() {
        if !partner_state.pending_locks.locks.contains(&lock.encoded) {
            return Err(message.clone().into());
        }
    }
    for lock in our_state.secrethashes_to_lockedlocks.values() {
        if !our_state.pending_locks.locks.contains(&lock.encoded) {
            return Err(message.clone().into());
        }
    }
    for lock in our_state.secrethashes_to_unlockedlocks.values() {
        if !our_state.pending_locks.locks.contains(&lock.encoded) {
            return Err(message.clone().into());
        }
    }
    for lock in our_state.secrethashes_to_onchain_unlockedlocks.values() {
        if !our_state.pending_locks.locks.contains(&lock.encoded) {
            return Err(message.clone().into());
        }
    }

    Ok(transition)
}

pub fn state_transition(
    channel_state: ChannelState,
    state_change: StateChange,
    block_number: BlockNumber,
    block_hash: BlockHash,
    pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
    let transition = match state_change {
        StateChange::ActionChannelClose(inner) => handle_action_close(channel_state, inner, block_number, block_hash),
        StateChange::ActionChannelWithdraw(inner) => {
            handle_action_withdraw(channel_state, inner, block_number, pseudo_random_number_generator)
        }
        StateChange::ActionChannelCoopSettle(inner) => {
            handle_action_coop_settle(channel_state, inner, block_number, pseudo_random_number_generator)
        }
        StateChange::ActionChannelSetRevealTimeout(inner) => {
            handle_action_set_channel_reveal_timeout(channel_state, inner)
        }
        StateChange::Block(inner) => handle_block(channel_state, inner, block_number, pseudo_random_number_generator),
        StateChange::ContractReceiveChannelClosed(inner) => handle_channel_closed(channel_state, inner),
        StateChange::ContractReceiveChannelSettled(inner) => handle_channel_settled(channel_state, inner),
        StateChange::ContractReceiveChannelDeposit(inner) => handle_channel_deposit(channel_state, inner),
        StateChange::ContractReceiveChannelWithdraw(inner) => handle_channel_withdraw(channel_state, inner),
        StateChange::ContractReceiveChannelBatchUnlock(inner) => handle_channel_batch_unlock(channel_state, inner),
        StateChange::ContractReceiveUpdateTransfer(inner) => {
            handle_channel_update_transfer(channel_state, inner, block_number)
        }
        StateChange::ReceiveWithdrawRequest(inner) => handle_receive_withdraw_request(
            channel_state,
            inner,
            block_number,
            block_hash,
            pseudo_random_number_generator,
        ),
        StateChange::ReceiveWithdrawConfirmation(inner) => {
            handle_receive_withdraw_confirmation(channel_state, inner, block_number, block_hash)
        }
        StateChange::ReceiveWithdrawExpired(inner) => {
            handle_receive_withdraw_expired(channel_state, inner, block_number)
        }
        _ => Err(StateTransitionError {
            msg: String::from("Could not transition channel"),
        }),
    }?;

    sanity_check(transition)
}