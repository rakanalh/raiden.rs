use thiserror::Error;

#[derive(Error, Display, Debug)]
pub struct MessageError {}

pub struct MessageHandler {}

impl MessageHandler {
	pub fn handle(&self, message: String) -> Result<(), MessageError> {
		Ok(())
	}
}
