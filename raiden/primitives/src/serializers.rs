use serde::Serialize;

use crate::types::ChainID;

impl Serialize for ChainID {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		let value: u64 = self.clone().into();
		serializer.serialize_str(&value.to_string())
	}
}
