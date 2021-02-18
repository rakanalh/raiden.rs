use std::sync::Arc;

use parking_lot::RwLock;
use raiden::{
    blockchain::contracts::ContractRegistry,
    state_machine::types::Event,
    state_manager::StateManager,
};

pub struct EventHandler {
    state_manager: Arc<RwLock<StateManager>>,
    contracts_registry: Arc<RwLock<ContractRegistry>>,
}

impl EventHandler {
    pub fn new(state_manager: Arc<RwLock<StateManager>>, contracts_registry: Arc<RwLock<ContractRegistry>>) -> Self {
        Self {
            state_manager,
            contracts_registry,
        }
    }

    pub async fn handle_event(&self, event: Event) {
        match event {
            Event::TokenNetworkCreated(event) => {
                let token_network_address = event.token_network.address;
                let _ = self
                    .contracts_registry
                    .write()
                    .add_token_network(token_network_address.into(), event.block_number.into());
            }
        }
    }
}
