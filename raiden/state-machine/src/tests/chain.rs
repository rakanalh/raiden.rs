use raiden_primitives::types::{
	H256,
	U256,
	U64,
};

use crate::{
	machine::chain,
	tests::factories::empty_chain_state,
	types::Block,
};

#[test]
fn chain_state_new_block() {
	let chain_state = empty_chain_state();
	let state_change =
		Block { block_number: U64::from(2u64), block_hash: H256::zero(), gas_limit: U256::zero() };
	let result = chain::state_transition(chain_state, state_change.into())
		.expect("State transition should succeed");
	assert_eq!(result.new_state.block_number, U64::from(2u64));

	let state_change =
		Block { block_number: U64::from(3u64), block_hash: H256::zero(), gas_limit: U256::zero() };
	let result = chain::state_transition(result.new_state, state_change.into())
		.expect("State transition should succeed");
	assert_eq!(result.new_state.block_number, U64::from(3u64));
}
