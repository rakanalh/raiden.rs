use std::{
    collections::HashMap,
    sync::Arc,
};
use ulid::Ulid;
use web3::types::{
    Address,
    H256,
    U64,
};

use crate::{
    errors::{self,},
    state_machine::{
        machine::chain,
        state::{
            ChainState,
            TokenNetworkRegistryState,
        },
        types::{
            ActionInitChain,
            ChainID,
            ContractReceiveTokenNetworkRegistry,
            Event,
            StateChange,
        },
    },
    storage::Storage,
};

pub type Result<T> = std::result::Result<T, errors::StateTransitionError>;

pub struct StateManager {
    pub storage: Arc<Storage>,
    pub current_state: ChainState,
}

impl StateManager {
    pub fn restore_or_init_state(
        storage: Arc<Storage>,
        chain_id: ChainID,
        our_address: Address,
        token_network_registry_address: Address,
        token_network_registry_deploy_block_number: U64,
    ) -> std::result::Result<Self, errors::RaidenError> {
        let snapshot = storage.get_snapshot_before_state_change(Ulid::from(u128::MAX));

        let (current_state, state_changes) = match snapshot {
            Ok(snapshot) => {
                // Load state changes since the snapshot's state_change_identifier
                // Set the snapshot
                // and then apply state_changes after
                let current_state =
                    serde_json::from_str(&snapshot.data).map_err(|e| errors::RaidenError { msg: format!("{}", e) })?;
                let state_changes_records = storage
                    .get_state_changes_in_range(snapshot.state_change_identifier, Ulid::from(u128::MAX).into())?;

                let mut state_changes = vec![];
                for record in state_changes_records {
                    let state_change = serde_json::from_str(&record.data)
                        .map_err(|e| errors::RaidenError { msg: format!("{}", e) })?;
                    state_changes.push(state_change);
                }
                (current_state, state_changes)
            }
            Err(_e) => Self::init_state(
                storage.clone(),
                chain_id,
                our_address,
                token_network_registry_address,
                token_network_registry_deploy_block_number,
            )?,
        };

        let mut state_manager = Self { storage, current_state };

        for state_change in state_changes {
            let _ = state_manager.dispatch(state_change);
        }

        Ok(state_manager)
    }

    fn init_state(
        storage: Arc<Storage>,
        chain_id: ChainID,
        our_address: Address,
        token_network_registry_address: Address,
        token_network_registry_deploy_block_number: U64,
    ) -> std::result::Result<(ChainState, Vec<StateChange>), errors::RaidenError> {
        let mut state_changes = vec![];

        let chain_state = ChainState {
            chain_id: chain_id.clone(),
            block_number: U64::from(1),
            our_address,
            identifiers_to_tokennetworkregistries: HashMap::new(),
        };

        state_changes.push(StateChange::ActionInitChain(ActionInitChain {
            chain_id,
            our_address,
            block_number: U64::from(1),
        }));

        let token_network_registry_state = TokenNetworkRegistryState::new(token_network_registry_address, vec![]);
        let new_network_registry_state_change = ContractReceiveTokenNetworkRegistry::new(
            H256::zero(),
            token_network_registry_state,
            token_network_registry_deploy_block_number,
            H256::zero(),
        );
        state_changes.push(StateChange::ContractReceiveTokenNetworkRegistry(
            new_network_registry_state_change,
        ));

        for record in storage.state_changes()? {
            let state_change =
                serde_json::from_str(&record.data).map_err(|e| errors::RaidenError { msg: format!("{}", e) })?;
            state_changes.push(state_change);
        }
        Ok((chain_state, state_changes))
    }

    fn dispatch(&mut self, state_change: StateChange) -> Result<Vec<Event>> {
        let current_state = self.current_state.clone();

        match chain::state_transition(current_state, state_change) {
            Ok(transition_result) => {
                self.current_state = transition_result.new_state;
                Ok(transition_result.events)
            }
            Err(e) => Err(errors::StateTransitionError {
                msg: format!("Could not transition: {}", e),
            }),
        }
    }

    pub fn transition(&mut self, state_change: StateChange) -> Result<Vec<Event>> {
        let state_change_id = match self.storage.store_state_change(state_change.clone()) {
            Ok(id) => Ok(id),
            Err(e) => Err(errors::StateTransitionError {
                msg: format!("Could not store state change: {}", e),
            }),
        }?;

        let events = self.dispatch(state_change.clone())?;

        if !events.is_empty() {
            match self.storage.store_events(state_change_id, events.clone()) {
                Ok(id) => Ok(id),
                Err(e) => Err(errors::StateTransitionError {
                    msg: format!("Could not store state change: {}", e),
                }),
            }?;
        }

        Ok(events)
    }
}
