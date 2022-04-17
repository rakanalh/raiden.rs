use crate::{
    primitives::U64,
    state_machine::{
        machine::chain,
        types::{
            Block,
            StateChange,
        },
    },
    tests::factories::empty_chain_state,
};
use web3::types::{
    H256,
    U256,
};

#[test]
fn chain_state_new_block() {
    let chain_state = empty_chain_state();
    let state_change = Block {
        block_number: U64::from(2u64),
        block_hash: H256::zero(),
        gas_limit: U256::zero(),
    };
    let result = chain::state_transition(chain_state, state_change.into()).expect("State transition should succeed");
    assert_eq!(result.new_state.block_number, U64::from(2u64));

    let state_change = Block {
        block_number: U64::from(3u64),
        block_hash: H256::zero(),
        gas_limit: U256::zero(),
    };
    let result =
        chain::state_transition(result.new_state, state_change.into()).expect("State transition should succeed");
    assert_eq!(result.new_state.block_number, U64::from(3u64));
}
