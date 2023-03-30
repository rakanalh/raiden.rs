use std::sync::Arc;

use parking_lot::RwLock;
use raiden_state_machine::types::StateChange;
use tracing::error;

use crate::{
	events::EventHandler,
	manager::StateManager,
};

pub mod events;
pub mod manager;
pub mod messages;

pub struct Transitioner {
	state_manager: Arc<RwLock<StateManager>>,
	event_handler: EventHandler,
}

impl Transitioner {
	pub fn new(state_manager: Arc<RwLock<StateManager>>, event_handler: EventHandler) -> Self {
		Self { state_manager, event_handler }
	}

	// TODO: Should return Result
	pub async fn transition(&self, state_change: StateChange) {
		let transition_result = self.state_manager.write().transition(state_change);
		match transition_result {
			Ok(events) =>
				for event in events {
					self.event_handler.handle_event(event).await;
				},
			Err(e) => {
				// Maybe use an informant service for error logging
				error!("Error transitioning: {:?}", e);
			},
		}
	}
}
