use std::{
	fmt,
	str::FromStr,
};

use serde::{
	de::{
		Error,
		Visitor,
	},
	Deserialize,
	Deserializer,
};
use web3::types::U256;

use crate::types::{
	ChainID,
	U64,
};

pub fn u256_from_u64<'de, D>(deserializer: D) -> Result<U256, D::Error>
where
	D: Deserializer<'de>,
{
	let buf = u64::deserialize(deserializer)?;
	Ok(U256::from(buf))
}

pub fn u256_from_str<'de, D>(deserializer: D) -> Result<U256, D::Error>
where
	D: Deserializer<'de>,
{
	let binding = serde_json::Value::deserialize(deserializer)?;
	let v = binding.as_str().ok_or_else(|| D::Error::custom("Could not parse U256"))?;
	Ok(U256::from_dec_str(v).map_err(|_| D::Error::custom("Invalid U256"))?)
}

pub fn u32_from_str<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
	D: Deserializer<'de>,
{
	let v: u64 = serde_json::Value::deserialize(deserializer)?
		.as_str()
		.and_then(|s| s.parse().ok())
		.ok_or_else(|| D::Error::custom("non-integer"))?;
	Ok(v as u32)
}

pub fn signature_from_str<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
	D: Deserializer<'de>,
{
	let v = serde_json::Value::deserialize(deserializer)?;
	let v = v
		.as_str()
		.ok_or_else(|| D::Error::custom("Invalid signature"))?
		.trim_start_matches("0x");
	let bytes = hex::decode(v).map_err(|_| D::Error::custom("Invalid signature"))?;
	Ok(bytes)
}

impl<'de> Deserialize<'de> for ChainID {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct IdVisitor;

		impl<'de> Visitor<'de> for IdVisitor {
			type Value = ChainID;

			fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
				f.write_str("Chain ID as a number or string")
			}

			fn visit_u64<E>(self, id: u64) -> Result<Self::Value, E>
			where
				E: Error,
			{
				Ok(id.into())
			}

			fn visit_str<E>(self, id: &str) -> Result<Self::Value, E>
			where
				E: Error,
			{
				Ok(ChainID::from_str(id)
					.map_err(|_| Error::custom("Could not parse ChainID from string"))?)
			}
		}

		deserializer.deserialize_any(IdVisitor)
	}
}

impl<'de> Deserialize<'de> for U64 {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct NumVisitor;

		impl<'de> Visitor<'de> for NumVisitor {
			type Value = U64;

			fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
				f.write_str("U64 as a number or string")
			}

			fn visit_u64<E>(self, num: u64) -> Result<Self::Value, E>
			where
				E: Error,
			{
				Ok(U64::from(num))
			}

			fn visit_str<E>(self, num: &str) -> Result<Self::Value, E>
			where
				E: Error,
			{
				Ok(U64::from_str(num)
					.map_err(|_| Error::custom("Could not parse U64 from string"))?)
			}
		}

		deserializer.deserialize_any(NumVisitor)
	}
}
