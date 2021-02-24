use std::sync::Arc;

use parking_lot::RwLock;
use raiden::{
    state_machine::types::StateChange,
    state_manager::StateManager,
};

use crate::event_handler::EventHandler;

pub struct TransitionService {
    state_manager: Arc<RwLock<StateManager>>,
    event_handler: EventHandler,
}

impl TransitionService {
    pub fn new(state_manager: Arc<RwLock<StateManager>>, event_handler: EventHandler) -> Self {
        Self {
            state_manager,
            event_handler,
        }
    }

    // TODO: Should return Result
    pub async fn transition(&self, state_change: StateChange) {
        let transition_result = self.state_manager.write().transition(state_change);
        match transition_result {
            Ok(events) => {
                for event in events {
                    self.event_handler.handle_event(event).await;
                }
            }
            Err(_e) => {
                // Maybe use an informant service for error logging
            }
        }
    }
}
