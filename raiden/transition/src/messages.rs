use std::sync::Arc;

use derive_more::Display;
use raiden_network_messages::messages::IncomingMessage;
use thiserror::Error;

use crate::Transitioner;

#[derive(Error, Display, Debug)]
pub struct MessageError {}

pub struct MessageHandler {
	transitioner: Arc<Transitioner>,
}

impl MessageHandler {
	pub fn handle(&self, _message: IncomingMessage) -> Result<(), MessageError> {
		// let state_change: StateChange = message.into();
		// self.transition_service.transition(state_change);
		Ok(())
	}
}
