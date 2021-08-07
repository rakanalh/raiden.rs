use std::sync::Arc;

use parking_lot::RwLock;
use raiden::{
    state_machine::types::Event,
    state_manager::StateManager,
};

pub struct EventHandler {
    _state_manager: Arc<RwLock<StateManager>>,
}

impl EventHandler {
    pub fn new(state_manager: Arc<RwLock<StateManager>>) -> Self {
        Self {
            _state_manager: state_manager,
        }
    }

    pub async fn handle_event(&self, _event: Event) {}
}
