use std::sync::Arc;

use raiden_primitives::types::CanonicalIdentifier;
use raiden_state_machine::{
	machine::chain,
	storage::{
		types::StorageID,
		StateStorage,
	},
	types::{
		ChainState,
		ChannelState,
	},
	views,
};

/// Retore state from existing storage, detect unapplied changes and apply them on top of found
/// snapshot.
fn restore_state(
	storage: Arc<StateStorage>,
	state_change_identifier: StorageID,
) -> Option<ChainState> {
	let snapshot = storage.get_snapshot_before_state_change(state_change_identifier).ok()?;
	let unapplied_state_changes = storage
		.get_state_changes_in_range(snapshot.state_change_identifier, state_change_identifier)
		.ok()?;

	let mut chain_state = snapshot.data;
	for state_change in unapplied_state_changes {
		let result = chain::state_transition(chain_state, state_change.data).ok()?;
		chain_state = result.new_state;
	}

	Some(chain_state)
}

/// Return a channel state before a state change was applied.
pub fn channel_state_until_state_change(
	storage: Arc<StateStorage>,
	canonical_identifier: CanonicalIdentifier,
	state_change_identifier: StorageID,
) -> Option<ChannelState> {
	let chain_state = restore_state(storage, state_change_identifier)?;
	views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier).cloned()
}
