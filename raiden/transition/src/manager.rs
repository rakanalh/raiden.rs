use std::sync::Arc;

use raiden_primitives::types::{
	Address,
	ChainID,
	TokenNetworkRegistryAddress,
	H256,
	U64,
};
use raiden_state_machine::{
	errors::StateTransitionError,
	machine::chain,
	types::{
		ActionInitChain,
		ChainState,
		ContractReceiveTokenNetworkRegistry,
		Event,
		StateChange,
		TokenNetworkRegistryState,
	},
};
use raiden_storage::{
	constants::SNAPSHOT_STATE_CHANGE_COUNT,
	errors::StorageError,
	state::StateStorage,
	types::StorageID,
};
use tracing::debug;

pub type Result<T> = std::result::Result<T, StateTransitionError>;

pub struct StateManager {
	pub storage: Arc<StateStorage>,
	pub current_state: ChainState,
	state_change_last_id: Option<StorageID>,
	state_change_count: u16,
}

impl StateManager {
	pub fn restore_or_init_state(
		storage: Arc<StateStorage>,
		chain_id: ChainID,
		our_address: Address,
		token_network_registry_address: TokenNetworkRegistryAddress,
		token_network_registry_deploy_block_number: U64,
	) -> std::result::Result<(Self, U64), StorageError> {
		let snapshot = storage.get_snapshot_before_state_change(u128::MAX.into());

		let (current_state, state_changes, block_number) = match snapshot {
			Ok(snapshot) => {
				// Load state changes since the snapshot's state_change_identifier
				// Set the snapshot
				// and then apply state_changes after
				debug!("Restoring state");
				let current_state: ChainState = snapshot.data;

				let state_changes_records = storage.get_state_changes_in_range(
					snapshot.state_change_identifier,
					u128::MAX.into(),
				)?;

				let mut state_changes = vec![];
				for record in state_changes_records {
					let state_change = record.data;
					state_changes.push(state_change);
				}
				let block_number = current_state.block_number;
				(current_state, state_changes, block_number)
			},
			Err(_e) => {
				debug!("Initializing state");
				Self::init_state(
					storage.clone(),
					chain_id,
					our_address,
					token_network_registry_address,
					token_network_registry_deploy_block_number,
				)?
			},
		};

		let mut state_manager =
			Self { storage, current_state, state_change_last_id: None, state_change_count: 0 };

		for state_change in state_changes {
			let _ = state_manager.dispatch(state_change);
		}

		Ok((state_manager, block_number))
	}

	fn init_state(
		storage: Arc<StateStorage>,
		chain_id: ChainID,
		our_address: Address,
		token_network_registry_address: TokenNetworkRegistryAddress,
		token_network_registry_deploy_block_number: U64,
	) -> std::result::Result<(ChainState, Vec<StateChange>, U64), StorageError> {
		let mut state_changes: Vec<StateChange> = vec![];

		let chain_state =
			ChainState::new(chain_id.clone(), U64::from(0), H256::zero(), our_address);

		state_changes.push(
			ActionInitChain {
				chain_id,
				our_address,
				block_number: U64::from(1),
				block_hash: H256::zero(),
			}
			.into(),
		);

		let token_network_registry_state =
			TokenNetworkRegistryState::new(token_network_registry_address, vec![]);
		let new_network_registry_state_change = ContractReceiveTokenNetworkRegistry {
			transaction_hash: Some(H256::zero()),
			token_network_registry: token_network_registry_state,
			block_number: token_network_registry_deploy_block_number,
			block_hash: H256::zero(),
		};
		state_changes.push(new_network_registry_state_change.into());

		for record in storage.state_changes()? {
			let state_change = record.data;
			state_changes.push(state_change);
		}
		Ok((chain_state, state_changes, token_network_registry_deploy_block_number))
	}

	fn dispatch(&mut self, state_change: StateChange) -> Result<Vec<Event>> {
		let current_state = self.current_state.clone();

		match chain::state_transition(current_state, state_change) {
			Ok(transition_result) => {
				self.current_state = transition_result.new_state;
				self.state_change_count += 1;
				self.maybe_snapshot();
				Ok(transition_result.events)
			},
			Err(e) => Err(StateTransitionError { msg: format!("Could not transition: {}", e) }),
		}
	}

	pub fn transition(&mut self, state_change: StateChange) -> Result<Vec<Event>> {
		let state_change_id = match self.storage.store_state_change(state_change.clone()) {
			Ok(id) => Ok(id),
			Err(e) =>
				Err(StateTransitionError { msg: format!("Could not store state change: {}", e) }),
		}?;

		let events = self.dispatch(state_change.clone())?;

		self.state_change_last_id = Some(state_change_id);

		if !events.is_empty() {
			match self.storage.store_events(state_change_id, events.clone()) {
				Ok(id) => Ok(id),
				Err(e) =>
					Err(StateTransitionError { msg: format!("Could not store event: {}", e) }),
			}?;
		}

		Ok(events)
	}

	fn maybe_snapshot(&mut self) {
		if self.state_change_count % SNAPSHOT_STATE_CHANGE_COUNT == 0 {
			return
		}
		let _ = self
			.storage
			.store_snapshot(self.current_state.clone(), self.state_change_last_id);
	}
}
