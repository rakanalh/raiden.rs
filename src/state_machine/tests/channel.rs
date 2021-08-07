use web3::types::{Address, H256, U256};

use crate::{primitives::{CanonicalIdentifier, U64}, state_machine::{machine::chain, types::{BalanceProofState, ContractReceiveChannelClosed, ContractSendChannelUpdateTransfer, ContractSendEvent, Event, StateChange}, views}, tests::factories::{
        chain_state_with_token_network,
        channel_state,
    }};

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

    let token_network_registry_state = chain_state.identifiers_to_tokennetworkregistries.get_mut(&token_network_registry_address).expect("Registry should exist");
    let token_network_state = token_network_registry_state.tokennetworkaddresses_to_tokennetworks.get_mut(&token_network_address).expect("token network should exist");
    let mut channel_state = token_network_state.channelidentifiers_to_channels.get_mut(&channel_identifier).expect("Channel should exist");
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
