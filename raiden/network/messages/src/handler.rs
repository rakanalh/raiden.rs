use std::sync::Arc;

use derive_more::Display;
use raiden_storage::state_transition::Transitioner;
use thiserror::Error;

use super::messages::Message;

#[derive(Error, Display, Debug)]
pub struct MessageError {}

pub struct MessageHandler {
	transition_service: Arc<dyn Transitioner>,
}

impl MessageHandler {
	pub fn handle(&self, _message: Message) -> Result<(), MessageError> {
		// let state_change: StateChange = message.into();
		// self.transition_service.transition(state_change);
		Ok(())
	}
}
