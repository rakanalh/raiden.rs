use serde::{
	Serialize,
	Serializer,
};

use crate::types::{
	ChainID,
	U64,
};

impl Serialize for ChainID {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		let value: u64 = self.clone().into();
		serializer.serialize_str(&value.to_string())
	}
}

pub fn u256_to_str<T, S>(v: &T, serializer: S) -> Result<S::Ok, S::Error>
where
	T: ToString,
	S: Serializer,
{
	serializer.serialize_str(&v.to_string())
}

impl Serialize for U64 {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_str(&self.to_string())
	}
}
