use std::collections::HashMap;

use crate::{
    constants::{
        DEFAULT_REVEAL_TIMEOUT,
        DEFAULT_SETTLE_TIMEOUT,
    },
    primitives::{
        CanonicalIdentifier,
        ChainID,
        Random,
        TokenAddress,
        TokenNetworkAddress,
        TokenNetworkRegistryAddress,
        TransactionExecutionStatus,
        TransactionResult,
        U64,
    },
    state_machine::{
        machine::chain,
        types::{
            ChainState,
            ChannelEndState,
            ChannelState,
            ContractReceiveChannelOpened,
            ContractReceiveTokenNetworkCreated,
            ContractReceiveTokenNetworkRegistry,
            FeeScheduleState,
            PaymentMappingState,
            StateChange,
            TokenNetworkGraphState,
            TokenNetworkRegistryState,
            TokenNetworkState,
        },
    },
};

use web3::types::{
    Address,
    H256,
    U256,
};

pub fn empty_chain_state() -> ChainState {
    ChainState {
        chain_id: ChainID::Goerli,
        block_number: U64::from(1u64),
        block_hash: H256::zero(),
        our_address: Address::random(),
        identifiers_to_tokennetworkregistries: HashMap::new(),
        queueids_to_queues: HashMap::new(),
        payment_mapping: PaymentMappingState {
            secrethashes_to_task: HashMap::new(),
        },
        pseudo_random_number_generator: &mut Random::new(),
    }
}

pub fn chain_state_with_token_network_registry(
    token_network_registry_address: TokenNetworkRegistryAddress,
) -> ChainState {
    let chain_state = empty_chain_state();
    let state_change = StateChange::ContractReceiveTokenNetworkRegistry(ContractReceiveTokenNetworkRegistry {
        transaction_hash: Some(H256::random()),
        token_network_registry: TokenNetworkRegistryState {
            address: token_network_registry_address,
            tokennetworkaddresses_to_tokennetworks: HashMap::new(),
            tokenaddresses_to_tokennetworkaddresses: HashMap::new(),
        },
        block_number: U64::from(1u64),
        block_hash: H256::random(),
    });

    let result = chain::state_transition(chain_state, state_change).expect("State transition should succeed");
    assert!(result
        .new_state
        .identifiers_to_tokennetworkregistries
        .get(&token_network_registry_address)
        .is_some());

    result.new_state
}

pub fn chain_state_with_token_network(
    token_network_registry_address: TokenNetworkRegistryAddress,
    token_address: TokenAddress,
    token_network_address: TokenNetworkAddress,
) -> ChainState {
    let chain_state = chain_state_with_token_network_registry(token_network_registry_address);

    let state_change = StateChange::ContractReceiveTokenNetworkCreated(ContractReceiveTokenNetworkCreated {
        transaction_hash: Some(H256::random()),
        token_network_registry_address,
        token_network: TokenNetworkState {
            address: token_network_address,
            token_address,
            network_graph: TokenNetworkGraphState {},
            channelidentifiers_to_channels: HashMap::new(),
            partneraddresses_to_channelidentifiers: HashMap::new(),
        },
        block_number: U64::from(1u64),
        block_hash: H256::random(),
    });
    let result = chain::state_transition(chain_state, state_change).expect("State transition should succeed");
    result.new_state
}

pub fn channel_state(
    chain_state: ChainState,
    token_network_registry_address: TokenNetworkRegistryAddress,
    token_network_address: TokenNetworkAddress,
    token_address: TokenAddress,
    channel_identifier: U256,
) -> ChainState {
    let state_change = StateChange::ContractReceiveChannelOpened(ContractReceiveChannelOpened {
        transaction_hash: Some(H256::random()),
        block_number: U64::from(1u64),
        block_hash: H256::random(),
        channel_state: ChannelState {
            canonical_identifier: CanonicalIdentifier {
                chain_identifier: chain_state.chain_id.clone(),
                token_network_address,
                channel_identifier,
            },
            token_address,
            token_network_registry_address,
            reveal_timeout: U64::from(DEFAULT_REVEAL_TIMEOUT),
            settle_timeout: U64::from(DEFAULT_SETTLE_TIMEOUT),
            fee_schedule: FeeScheduleState::default(),
            our_state: empty_channel_end_state(chain_state.our_address),
            partner_state: empty_channel_end_state(Address::random()),
            open_transaction: TransactionExecutionStatus {
                started_block_number: Some(U64::from(1u64)),
                finished_block_number: Some(U64::from(2u64)),
                result: Some(TransactionResult::Success),
            },
            close_transaction: None,
            settle_transaction: None,
            update_transaction: None,
        },
    });
    let result = chain::state_transition(chain_state, state_change).expect("channel creation should work");
    result.new_state
}

pub fn empty_channel_end_state(address: Address) -> ChannelEndState {
    ChannelEndState {
        address,
        ..ChannelEndState::default()
    }
}
