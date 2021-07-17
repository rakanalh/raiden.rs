use serde::{
    Deserialize,
    Serialize,
};
use std::ops::{
    Add,
    Deref,
    Mul,
};
use web3::types::{
    U256,
    U64 as PrimitiveU64,
};

#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct U64(PrimitiveU64);

impl std::fmt::Display for U64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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

impl Add<U64> for U64 {
    type Output = U64;

    fn add(self, rhs: U64) -> Self::Output {
        U64::from(self.0 + rhs.0)
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

impl Deref for U64 {
    type Target = PrimitiveU64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
