use derive_more::{
    Deref,
    Display,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    ops::{
        Add,
        Mul,
        Sub,
    },
    str::FromStr,
};
use web3::types::{
    U256,
    U64 as PrimitiveU64,
};

use super::TokenAmount;

#[derive(Default, Copy, Clone, Display, Debug, Deref, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize, Deserialize)]
pub struct U64(PrimitiveU64);

impl U64 {
    pub fn zero() -> Self {
        Self(PrimitiveU64::zero())
    }

    pub fn as_bytes(&self) -> &[u8] {
        let mut bytes = vec![];
        self.0.to_big_endian(&mut bytes);

        let r: &mut [u8] = Default::default();
        r.clone_from_slice(&bytes[..]);
        r
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
        Ok(U64(PrimitiveU64::from_str(s).map_err(|_| ())?))
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

pub trait AmountToBytes {
    fn to_bytes(&self) -> &[u8];
}

impl AmountToBytes for TokenAmount {
    fn to_bytes(&self) -> &[u8] {
        let mut bytes = vec![];
        self.to_big_endian(&mut bytes);

        let r: &mut [u8] = Default::default();
        r.clone_from_slice(&bytes[..]);
        r
    }
}
