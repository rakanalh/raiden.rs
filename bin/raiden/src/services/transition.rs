use std::sync::Arc;

use futures::{StreamExt, channel::mpsc};
use parking_lot::RwLock;
use raiden::{state_machine::types::StateChange, state_manager::StateManager};

use crate::event_handler::EventHandler;

pub struct TransitionService {
	state_manager: Arc<RwLock<StateManager>>,
	event_handler: EventHandler,
	state_changes: mpsc::Receiver<StateChange>,
}

impl TransitionService {
	pub fn new(
		state_manager: Arc<RwLock<StateManager>>,
		event_handler: EventHandler,
	) -> (Self, mpsc::Sender<StateChange>) {
		let (sender, receiver) = mpsc::channel(16);

		let service = Self {
			state_manager,
			event_handler,
			state_changes: receiver,
		};
		(service, sender)
	}

	pub async fn start(mut self) {
		loop {
			let state_change = match self.state_changes.next().await {
				Some(sc) => sc,
				None => continue,
			};

			let transition_result = self.state_manager.write().transition(state_change);
			match transition_result {
				Ok(events) => {
					for event in events {
						self.event_handler.handle_event(event).await;
					}
				}
				Err(_e) => {
					// Maybe use an informant service for error logging
				},
			}
		}

	}
}
