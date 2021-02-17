use std::sync::Arc;
use ulid::Ulid;
use web3::types::{
    Address,
    H256,
    U64,
};

use crate::{
    errors::{
        self,
        RaidenError,
    },
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
    pub current_state: Option<ChainState>,
}

impl StateManager {
    pub fn new(storage: Arc<Storage>) -> StateManager {
        StateManager {
            storage,
            current_state: None,
        }
    }

    pub fn setup(&self) -> std::result::Result<(), errors::RaidenError> {
        self.storage.setup_database().map_err(|e| e.into())
    }

    pub fn restore_or_init_state(
        &mut self,
        chain_id: ChainID,
        our_address: Address,
        token_network_registry_address: Address,
        token_network_registry_deploy_block_number: U64,
    ) -> std::result::Result<(), errors::RaidenError> {
        let snapshot = self.storage.get_snapshot_before_state_change(Ulid::from(u128::MAX));

        let last_state_change_id = match snapshot {
            Ok(snapshot) => {
                // Load state changes since the snapshot's state_change_identifier
                // Set the snapshot
                // and then apply state_changes after
                self.current_state = Some(
                    serde_json::from_str(&snapshot.data).map_err(|e| errors::RaidenError { msg: format!("{}", e) })?,
                );
                snapshot.state_change_identifier
            }
            Err(_e) => {
                self.init_state(
                    chain_id,
                    our_address,
                    token_network_registry_address,
                    token_network_registry_deploy_block_number,
                )?;
                Ulid::from(u128::MIN).into()
            }
        };

        let state_changes_records = self
            .storage
            .get_state_changes_in_range(last_state_change_id, Ulid::from(u128::MAX).into())?;
        for state_change_record in state_changes_records {
            let state_change = serde_json::from_str(&state_change_record.data)
                .map_err(|e| errors::RaidenError { msg: format!("{}", e) })?;
            let _ = self.dispatch(state_change);
        }

        Ok(())
    }

    fn init_state(
        &mut self,
        chain_id: ChainID,
        our_address: Address,
        token_network_registry_address: Address,
        token_network_registry_deploy_block_number: U64,
    ) -> std::result::Result<(), errors::RaidenError> {
        let init_chain = ActionInitChain {
            chain_id,
            our_address,
            block_number: U64::from(1),
        };

        self.dispatch(StateChange::ActionInitChain(init_chain))
            .map_err(|e| RaidenError { msg: format!("{}", e) })?;

        let token_network_registry_state = TokenNetworkRegistryState::new(token_network_registry_address, vec![]);
        let new_network_registry_state_change = ContractReceiveTokenNetworkRegistry::new(
            H256::zero(),
            token_network_registry_state,
            token_network_registry_deploy_block_number,
            H256::zero(),
        );
        self.dispatch(StateChange::ContractReceiveTokenNetworkRegistry(
            new_network_registry_state_change,
        ))
        .map_err(|e| RaidenError { msg: format!("{}", e) })?;

        let state_changes_records = self.storage.state_changes()?;
        for state_change_record in state_changes_records {
            let state_change = serde_json::from_str(&state_change_record.data)
                .map_err(|e| errors::RaidenError { msg: format!("{}", e) })?;
            let _ = self.dispatch(state_change);
        }
        Ok(())
    }

    fn dispatch(&mut self, state_change: StateChange) -> Result<Vec<Event>> {
        let current_state = self.current_state.clone();

        match chain::state_transition(current_state, state_change) {
            Ok(transition_result) => {
                self.current_state.replace(transition_result.new_state);
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
