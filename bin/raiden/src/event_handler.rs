use std::sync::Arc;

use parking_lot::RwLock;
use raiden::{
    state_machine::types::Event,
    state_manager::StateManager,
};

pub struct EventHandler {
    state_manager: Arc<RwLock<StateManager>>,
}

impl EventHandler {
    pub fn new(state_manager: Arc<RwLock<StateManager>>) -> Self {
        Self { state_manager }
    }

    pub async fn handle_event(&self, event: Event) {
        match event {
            Event::TokenNetworkCreated(event) => {
                let _token_network_address = event.token_network.address;
            }
        }
    }
}
