use std::sync::Arc;

use futures::Future;
use parking_lot::RwLock;
use raiden_state_machine::types::{
	Event,
	StateChange,
};

use crate::state_manager::StateManager;

#[async_trait::async_trait]
pub trait Transitioner {
	async fn transition(&self, state_change: StateChange);
}

pub struct TransitionService<F, Fut>
where
	F: Fn(Event) -> Fut,
	Fut: Future<Output = ()>,
{
	state_manager: Arc<RwLock<StateManager>>,
	event_handler: F,
}

impl<F, Fut> TransitionService<F, Fut>
where
	F: Fn(Event) -> Fut,
	Fut: Future<Output = ()>,
{
	pub fn new(state_manager: Arc<RwLock<StateManager>>, event_handler: F) -> Self {
		Self { state_manager, event_handler }
	}
}

#[async_trait::async_trait]
impl<F, Fut> Transitioner for TransitionService<F, Fut>
where
	F: Fn(Event) -> Fut + Send + Sync,
	Fut: Future<Output = ()> + Send,
{
	// TODO: Should return Result
	async fn transition(&self, state_change: StateChange) {
		let transition_result = self.state_manager.write().transition(state_change);
		match transition_result {
			Ok(events) =>
				for event in events {
					(self.event_handler)(event).await;
				},
			Err(e) => {
				// Maybe use an informant service for error logging
				println!("Error transitioning: {:?}", e);
			},
		}
	}
}
