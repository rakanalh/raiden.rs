#![warn(clippy::missing_docs_in_private_items)]

use std::{
	ops::{
		Add,
		Mul,
		Sub,
	},
	str::FromStr,
};

use derive_more::Display;
use web3::types::{
	U256,
	U64 as PrimitiveU64,
};

/// A wrapper around web3's U64 types for consistency.
#[derive(
	Default, Copy, Clone, Display, Debug, derive_more::Deref, Eq, Ord, PartialEq, PartialOrd, Hash,
)]
pub struct U64(PrimitiveU64);

impl U64 {
	/// Return zero value.
	pub fn zero() -> Self {
		Self(PrimitiveU64::zero())
	}

	/// Convert to bytes.
	pub fn as_bytes(&self) -> Vec<u8> {
		let mut bytes: [u8; 8] = [0; 8];
		self.0.to_big_endian(&mut bytes);
		bytes.to_vec()
	}

	/// Convert to big endian bytes.
	pub fn to_be_bytes(&self) -> Vec<u8> {
		let bytes = self.as_bytes();
		let mut padded_bytes: [u8; 32] = [0; 32];
		padded_bytes[24..].copy_from_slice(&bytes);
		padded_bytes.to_vec()
	}
}

impl From<PrimitiveU64> for U64 {
	fn from(n: PrimitiveU64) -> Self {
		Self(n)
	}
}

impl From<U64> for PrimitiveU64 {
	fn from(n: U64) -> Self {
		n.0
	}
}

impl FromStr for U64 {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if let Ok(num) = PrimitiveU64::from_dec_str(s) {
			return Ok(U64(num))
		}
		let num = PrimitiveU64::from_str(s).map_err(|_| ())?;
		Ok(U64(num))
	}
}

impl Add<U64> for U64 {
	type Output = U64;

	fn add(self, rhs: U64) -> Self::Output {
		U64::from(self.0 + rhs.0)
	}
}

impl Sub<U64> for U64 {
	type Output = U64;

	fn sub(self, rhs: U64) -> Self::Output {
		U64::from(self.0 - rhs.0)
	}
}

impl Mul<U64> for U64 {
	type Output = U64;

	fn mul(self, rhs: U64) -> Self::Output {
		U64::from(self.0 * rhs.0)
	}
}

impl Mul<u64> for U64 {
	type Output = U64;

	fn mul(self, rhs: u64) -> Self::Output {
		U64::from(self.0 * rhs)
	}
}

impl From<U64> for U256 {
	fn from(num: U64) -> Self {
		num.0.low_u64().into()
	}
}

impl From<u64> for U64 {
	fn from(n: u64) -> Self {
		Self(n.into())
	}
}

impl From<u32> for U64 {
	fn from(n: u32) -> Self {
		Self((n as u64).into())
	}
}

impl From<i32> for U64 {
	fn from(n: i32) -> Self {
		Self((n as u64).into())
	}
}
