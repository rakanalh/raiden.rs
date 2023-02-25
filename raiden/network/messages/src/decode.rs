use std::collections::HashMap;

use raiden_state_machine::types::AddressMetadata;

use super::messages::{
	LockedTransfer,
	Message,
};

pub struct MessageDecoder {}

impl MessageDecoder {
	pub fn decode(body: serde_json::Value) -> Result<Message, ()> {
		let s = body.as_str().unwrap().to_owned();
		println!("Decoding {:?}", s);
		println!("");
		println!("");
		let map: HashMap<String, serde_json::Value> = serde_json::from_str(&s).unwrap();
		println!("Into Hashmap{:?}", map);
		println!("");
		println!("");
		if map.get("type").unwrap() == "LockedTransfer" {
			let locked_transfer: LockedTransfer = serde_json::from_str(&s).unwrap();
			return Ok(Message {
				message_identifier: locked_transfer.message_identifier,
				recipient: locked_transfer.recipient,
				recipient_metadata: AddressMetadata {
					user_id: "".to_owned(),
					displayname: "".to_owned(),
					capabilities: "".to_owned(),
				},
				inner: crate::messages::MessageInner::LockedTransfer(locked_transfer),
			})
		}
		Err(())
	}
}
