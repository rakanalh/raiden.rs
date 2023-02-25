use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::traits::ToBytes;
use raiden_state_machine::types::SendProcessed;
use serde::{
	Deserialize,
	Serialize,
};
use web3::signing::SigningError;

use super::{
	CmdId,
	SignedMessage,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Processed {
	pub message_identifier: u32,
	pub signature: Vec<u8>,
}

impl From<SendProcessed> for Processed {
	fn from(event: SendProcessed) -> Self {
		Self { message_identifier: event.message_identifier, signature: vec![] }
	}
}

impl SignedMessage for Processed {
	fn bytes(&self) -> Vec<u8> {
		let cmd_id: [u8; 1] = CmdId::Processed.into();

		let mut bytes = vec![];
		bytes.extend_from_slice(&cmd_id);
		bytes.extend_from_slice(&[0, 0, 0]);
		bytes.extend_from_slice(&self.message_identifier.to_be_bytes());
		bytes
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.as_vec();
		Ok(())
	}
}
