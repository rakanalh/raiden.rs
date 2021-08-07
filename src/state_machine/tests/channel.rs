use web3::types::{
    Address,
    H256,
    U256,
};

use crate::{
    primitives::{
        CanonicalIdentifier,
        MediationFeeConfig,
        TransactionExecutionStatus,
        TransactionResult,
        U64,
    },
    state_machine::{
        machine::chain,
        types::{
            BalanceProofState,
            Block,
            ContractReceiveChannelClosed,
            ContractReceiveChannelDeposit,
            ContractReceiveChannelWithdraw,
            ContractSendChannelSettle,
            ContractSendChannelUpdateTransfer,
            ContractSendEvent,
            Event,
            PendingWithdrawState,
            SendMessageEventInner,
            SendWithdrawExpired,
            StateChange,
            TransactionChannelDeposit,
        },
        views,
    },
    tests::factories::{
        chain_state_with_token_network,
        channel_state,
    },
};

#[test]
fn test_open_channel_new_block_with_expired_withdraws() {
    let token_network_registry_address = Address::random();
    let token_address = Address::random();
    let token_network_address = Address::random();

    let chain_state =
        chain_state_with_token_network(token_network_registry_address, token_address, token_network_address);
    let channel_identifier = U256::from(1u64);

    let mut chain_state = channel_state(
        chain_state,
        token_network_registry_address,
        token_network_address,
        token_address,
        channel_identifier,
    );

    let chain_identifier = chain_state.chain_id.clone();
    let canonical_identifier = CanonicalIdentifier {
        chain_identifier,
        token_network_address,
        channel_identifier,
    };

    let token_network_registry_state = chain_state
        .identifiers_to_tokennetworkregistries
        .get_mut(&token_network_registry_address)
        .expect("Registry should exist");
    let token_network_state = token_network_registry_state
        .tokennetworkaddresses_to_tokennetworks
        .get_mut(&token_network_address)
        .expect("token network should exist");
    let channel_state = token_network_state
        .channelidentifiers_to_channels
        .get_mut(&channel_identifier)
        .expect("Channel should exist");

    channel_state.our_state.withdraws_pending.insert(
        U256::from(100u64),
        PendingWithdrawState {
            total_withdraw: U256::from(100u64),
            expiration: U64::from(50u64),
            nonce: U256::from(1),
            recipient_metadata: None,
        },
    );

    let expected_event = Event::SendWithdrawExpired(SendWithdrawExpired {
        inner: SendMessageEventInner {
            recipient: channel_state.partner_state.address.clone(),
            recipient_metadata: None,
            canonincal_identifier: canonical_identifier.clone(),
            message_identifier: 1,
        },
        participant: channel_state.our_state.address.clone(),
        nonce: U256::from(1u64),
        expiration: U64::from(50u64),
    });
    let state_change = StateChange::Block(Block {
        block_number: U64::from(511u64),
        block_hash: H256::random(),
        gas_limit: U256::zero(),
    });
    let result = chain::state_transition(chain_state.clone(), state_change).expect("Block should succeed");

    assert!(!result.events.is_empty());
    assert_eq!(result.events[0], expected_event,)
}

#[test]
fn test_closed_channel_new_block() {
    let token_network_registry_address = Address::random();
    let token_address = Address::random();
    let token_network_address = Address::random();

    let chain_state =
        chain_state_with_token_network(token_network_registry_address, token_address, token_network_address);

    let channel_identifier = U256::from(1u64);
    let chain_state = channel_state(
        chain_state,
        token_network_registry_address,
        token_network_address,
        token_address,
        channel_identifier,
    );
    let channel_identifier = U256::from(1u64);
    let mut chain_state = channel_state(
        chain_state,
        token_network_registry_address,
        token_network_address,
        token_address,
        channel_identifier,
    );

    let chain_identifier = chain_state.chain_id.clone();
    let canonical_identifier = CanonicalIdentifier {
        chain_identifier,
        token_network_address,
        channel_identifier,
    };

    let token_network_registry_state = chain_state
        .identifiers_to_tokennetworkregistries
        .get_mut(&token_network_registry_address)
        .expect("Registry should exist");
    let token_network_state = token_network_registry_state
        .tokennetworkaddresses_to_tokennetworks
        .get_mut(&token_network_address)
        .expect("token network should exist");
    let mut channel_state = token_network_state
        .channelidentifiers_to_channels
        .get_mut(&channel_identifier)
        .expect("Channel should exist");

    channel_state.close_transaction = Some(TransactionExecutionStatus {
        started_block_number: Some(U64::from(10u64)),
        finished_block_number: Some(U64::from(10u64)),
        result: Some(TransactionResult::Success),
    });

    let block_hash = H256::random();
    let state_change = StateChange::Block(Block {
        block_number: U64::from(511u64),
        block_hash,
        gas_limit: U256::zero(),
    });
    let result = chain::state_transition(chain_state, state_change).expect("Block should succeed");

    assert!(!result.events.is_empty());
    assert_eq!(
        result.events[0],
        Event::ContractSendChannelSettle(ContractSendChannelSettle {
            inner: ContractSendEvent {
                triggered_by_blockhash: block_hash,
            },
            canonical_identifier: canonical_identifier.clone(),
        })
    );
}

#[test]
fn test_channel_opened() {
    let token_network_registry_address = Address::random();
    let token_address = Address::random();
    let token_network_address = Address::random();

    let chain_state =
        chain_state_with_token_network(token_network_registry_address, token_address, token_network_address);

    let channel_identifier = U256::from(1u64);
    let chain_state = channel_state(
        chain_state,
        token_network_registry_address,
        token_network_address,
        token_address,
        channel_identifier,
    );

    let chain_identifier = chain_state.chain_id.clone();
    let channel_state = views::get_channel_by_canonical_identifier(
        &chain_state,
        CanonicalIdentifier {
            chain_identifier,
            token_network_address,
            channel_identifier,
        },
    );

    assert!(channel_state.is_some());
}

#[test]
fn test_channel_closed() {
    let token_network_registry_address = Address::random();
    let token_address = Address::random();
    let token_network_address = Address::random();

    let chain_state =
        chain_state_with_token_network(token_network_registry_address, token_address, token_network_address);

    let channel_identifier = U256::from(1u64);
    let chain_state = channel_state(
        chain_state,
        token_network_registry_address,
        token_network_address,
        token_address,
        channel_identifier,
    );

    let chain_identifier = chain_state.chain_id.clone();
    let canonical_identifier = CanonicalIdentifier {
        chain_identifier: chain_identifier.clone(),
        token_network_address,
        channel_identifier,
    };
    let state_change = StateChange::ContractReceiveChannelClosed(ContractReceiveChannelClosed {
        transaction_hash: Some(H256::random()),
        block_number: U64::from(10u64),
        block_hash: H256::random(),
        transaction_from: Address::random(),
        canonical_identifier: canonical_identifier.clone(),
    });

    let result = chain::state_transition(chain_state.clone(), state_change.clone()).expect("Should close channel");
    assert!(result.events.is_empty());

    let channel_identifier = U256::from(2u64);
    let canonical_identifier = CanonicalIdentifier {
        chain_identifier: chain_identifier.clone(),
        token_network_address,
        channel_identifier,
    };
    let mut chain_state = channel_state(
        chain_state,
        token_network_registry_address,
        token_network_address,
        token_address,
        channel_identifier,
    );

    let balance_proof_state = BalanceProofState {
        nonce: U256::from(1u64),
        transferred_amount: U256::zero(),
        locked_amount: U256::zero(),
        locksroot: H256::default(),
        canonical_identifier: canonical_identifier.clone(),
        balance_hash: H256::zero(),
        message_hash: Some(H256::zero()),
        signature: None,
        sender: Some(Address::zero()),
    };

    let token_network_registry_state = chain_state
        .identifiers_to_tokennetworkregistries
        .get_mut(&token_network_registry_address)
        .expect("Registry should exist");
    let token_network_state = token_network_registry_state
        .tokennetworkaddresses_to_tokennetworks
        .get_mut(&token_network_address)
        .expect("token network should exist");
    let mut channel_state = token_network_state
        .channelidentifiers_to_channels
        .get_mut(&channel_identifier)
        .expect("Channel should exist");
    channel_state.partner_state.balance_proof = Some(balance_proof_state.clone());

    let state_change = StateChange::ContractReceiveChannelClosed(ContractReceiveChannelClosed {
        transaction_hash: Some(H256::random()),
        block_number: U64::from(10u64),
        block_hash: H256::zero(),
        transaction_from: Address::random(),
        canonical_identifier: canonical_identifier.clone(),
    });

    let result = chain::state_transition(chain_state.clone(), state_change.clone()).expect("Should close channel");

    let event = Event::ContractSendChannelUpdateTransfer(ContractSendChannelUpdateTransfer {
        inner: ContractSendEvent {
            triggered_by_blockhash: H256::zero(),
        },
        expiration: U256::from(510u64),
        balance_proof: balance_proof_state,
    });
    assert!(!result.events.is_empty());
    assert_eq!(result.events[0], event);
}

#[test]
fn test_channel_withdraw() {
    let token_network_registry_address = Address::random();
    let token_address = Address::random();
    let token_network_address = Address::random();

    let chain_state =
        chain_state_with_token_network(token_network_registry_address, token_address, token_network_address);

    let channel_identifier = U256::from(1u64);
    let chain_identifier = chain_state.chain_id.clone();
    let canonical_identifier = CanonicalIdentifier {
        chain_identifier: chain_identifier.clone(),
        token_network_address,
        channel_identifier,
    };
    let chain_state = channel_state(
        chain_state,
        token_network_registry_address,
        token_network_address,
        token_address,
        channel_identifier,
    );

    let channel_state = views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
        .expect("Channel should exist");

    assert_eq!(channel_state.our_state.contract_balance, U256::zero());

    let state_change = StateChange::ContractReceiveChannelWithdraw(ContractReceiveChannelWithdraw {
        canonical_identifier: canonical_identifier.clone(),
        participant: chain_state.our_address.clone(),
        total_withdraw: U256::from(100u64),
        fee_config: MediationFeeConfig::default(),
    });
    let result = chain::state_transition(chain_state, state_change).expect("Withdraw should succeed");
    let chain_state = result.new_state;
    let channel_state = views::get_channel_by_canonical_identifier(&chain_state.clone(), canonical_identifier.clone())
        .expect("Channel should exist")
        .clone();
    assert_eq!(channel_state.our_state.onchain_total_withdraw, U256::from(100u64));

    let state_change = StateChange::ContractReceiveChannelWithdraw(ContractReceiveChannelWithdraw {
        canonical_identifier: canonical_identifier.clone(),
        participant: channel_state.partner_state.address,
        total_withdraw: U256::from(99u64),
        fee_config: MediationFeeConfig::default(),
    });
    let result = chain::state_transition(chain_state, state_change).expect("Withdraw should succeed");
    let chain_state = result.new_state;
    let channel_state = views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
        .expect("Channel should exist");
    assert_eq!(channel_state.partner_state.onchain_total_withdraw, U256::from(99u64));
}

#[test]
fn test_channel_deposit() {
    let token_network_registry_address = Address::random();
    let token_address = Address::random();
    let token_network_address = Address::random();

    let chain_state =
        chain_state_with_token_network(token_network_registry_address, token_address, token_network_address);

    let channel_identifier = U256::from(1u64);
    let chain_identifier = chain_state.chain_id.clone();
    let canonical_identifier = CanonicalIdentifier {
        chain_identifier: chain_identifier.clone(),
        token_network_address,
        channel_identifier,
    };
    let chain_state = channel_state(
        chain_state,
        token_network_registry_address,
        token_network_address,
        token_address,
        channel_identifier,
    );

    let channel_state = views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
        .expect("Channel should exist");

    assert_eq!(channel_state.our_state.contract_balance, U256::zero());

    let state_change = StateChange::ContractReceiveChannelDeposit(ContractReceiveChannelDeposit {
        canonical_identifier: canonical_identifier.clone(),
        deposit_transaction: TransactionChannelDeposit {
            participant_address: chain_state.our_address.clone(),
            contract_balance: U256::from(100u64),
            deposit_block_number: U64::from(10u64),
        },
        fee_config: MediationFeeConfig::default(),
    });
    let result = chain::state_transition(chain_state, state_change).expect("Deposit should succeed");
    let channel_state = views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone())
        .expect("Channel should exist");
    assert_eq!(channel_state.our_state.contract_balance, U256::from(100u64));

    let chain_state = result.new_state;
    let state_change = StateChange::ContractReceiveChannelDeposit(ContractReceiveChannelDeposit {
        canonical_identifier: canonical_identifier.clone(),
        deposit_transaction: TransactionChannelDeposit {
            participant_address: chain_state.our_address.clone(),
            contract_balance: U256::from(99u64), // Less than the deposit before
            deposit_block_number: U64::from(10u64),
        },
        fee_config: MediationFeeConfig::default(),
    });
    let result = chain::state_transition(chain_state, state_change).expect("Deposit should succeed");
    let channel_state = views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier)
        .expect("Channel should exist");
    assert_eq!(channel_state.our_state.contract_balance, U256::from(100u64));
}

#[test]
fn test_channel_settled() {}

#[test]
fn test_channel_batch_unlock() {}

#[test]
fn test_channel_update_transfer() {}

#[test]
fn test_channel_action_withdraw() {}

#[test]
fn test_channel_set_reveal_timeout() {}
