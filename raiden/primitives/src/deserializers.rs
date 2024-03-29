#![warn(clippy::missing_docs_in_private_items)]

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
use web3::types::{
	H256,
	U256,
};

use crate::types::{
	ChainID,
	Signature,
	U64,
};

/// Deserialize u64 into U256
pub fn u256_from_u64<'de, D>(deserializer: D) -> Result<U256, D::Error>
where
	D: Deserializer<'de>,
{
	let buf = u64::deserialize(deserializer)?;
	Ok(U256::from(buf))
}

/// Deserialize string to U256.
pub fn u256_from_str<'de, D>(deserializer: D) -> Result<U256, D::Error>
where
	D: Deserializer<'de>,
{
	let binding = serde_json::Value::deserialize(deserializer)?;
	if let Some(value) = binding.as_u64() {
		return Ok(U256::from(value))
	}
	let v = binding.as_str().ok_or_else(|| D::Error::custom("Could not parse U256"))?;
	U256::from_dec_str(v).map_err(|_| D::Error::custom("Invalid U256"))
}

/// Deserialize an optional string (none) into U256.
pub fn u256_from_optional_str<'de, D>(deserializer: D) -> Result<Option<U256>, D::Error>
where
	D: Deserializer<'de>,
{
	let binding = serde_json::Value::deserialize(deserializer)?;
	if let Some(value) = binding.as_u64() {
		return Ok(Some(U256::from(value)))
	}
	let v = binding.as_str().ok_or_else(|| D::Error::custom("Could not parse U256"))?;
	Ok(Some(U256::from_dec_str(v).map_err(|_| D::Error::custom("Invalid U256"))?))
}

/// Deserialize string to u64.
pub fn u64_from_str<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
	D: Deserializer<'de>,
{
	let value = serde_json::Value::deserialize(deserializer)?;
	let v = match value.as_u64() {
		Some(v) => v,
		None => value
			.as_str()
			.and_then(|s| s.parse().ok())
			.ok_or_else(|| D::Error::custom("non-integer"))?,
	};
	Ok(v)
}

/// Deserialize string into H256.
pub fn h256_from_str<'de, D>(deserializer: D) -> Result<H256, D::Error>
where
	D: Deserializer<'de>,
{
	let binding = serde_json::Value::deserialize(deserializer)?;
	let str_value = binding.as_str().ok_or_else(|| D::Error::custom("Could not parse H256"))?;
	let hex_value = hex::decode(str_value.trim_start_matches("0x"))
		.map_err(|e| D::Error::custom(format!("Could not decode hex: {:?}", e)))?;
	Ok(H256::from_slice(&hex_value))
}

/// Deserialize string to signature.
pub fn signature_from_str<'de, D>(deserializer: D) -> Result<Signature, D::Error>
where
	D: Deserializer<'de>,
{
	let v = serde_json::Value::deserialize(deserializer)?;
	let v = v
		.as_str()
		.ok_or_else(|| D::Error::custom("Invalid signature"))?
		.trim_start_matches("0x");
	Ok(Signature::from(hex::decode(v).map_err(|_| D::Error::custom("Invalid signature"))?))
}

impl<'de> Deserialize<'de> for ChainID {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		/// Visitor for Chain Identifier to try to parse from different types
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
				ChainID::from_str(id)
					.map_err(|_| Error::custom("Could not parse ChainID from string"))
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
		/// Visitor for U64 to try to parse from different types
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
				U64::from_str(num).map_err(|_| Error::custom("Could not parse U64 from string"))
			}
		}

		deserializer.deserialize_any(NumVisitor)
	}
}
