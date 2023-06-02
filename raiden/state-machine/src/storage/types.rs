#![warn(clippy::missing_docs_in_private_items)]

use std::convert::TryFrom;

use chrono::NaiveDateTime;
use derive_more::Display;
use ulid::{
	DecodeError,
	Ulid,
};

use crate::types::{
	ChainState,
	Event,
	StateChange,
};

/// Result of storage operation.
pub type Result<T> = std::result::Result<T, StorageError>;

/// Storage error type.
#[derive(Display, Debug)]
pub enum StorageError {
	#[display(fmt = "Storage lock poisoned")]
	CannotLock,
	#[display(fmt = "Cannot serialize for storage {}", _0)]
	SerializationError(serde_json::Error),
	#[display(fmt = "SQL Error: {}", _0)]
	Sql(rusqlite::Error),
	#[display(fmt = "Cannot convert value to Ulid: {}", _0)]
	ID(DecodeError),
	#[display(fmt = "Error: {}", _0)]
	Other(&'static str),
}

/// Storage record identifier
#[derive(Clone, Copy, Debug)]
pub struct StorageID {
	pub(crate) inner: Ulid,
}

impl StorageID {
	/// Returns zero value
	pub fn zero() -> Self {
		Self { inner: Ulid::nil() }
	}

	/// Returns max possible value.
	pub fn max() -> Self {
		Self { inner: u128::MAX.into() }
	}
}

impl std::fmt::Display for StorageID {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.inner.to_string())
	}
}

impl From<u128> for StorageID {
	fn from(id: u128) -> Self {
		Self { inner: Ulid::from(id) }
	}
}

impl From<Ulid> for StorageID {
	fn from(id: Ulid) -> Self {
		return Self { inner: id }
	}
}

impl Into<String> for StorageID {
	fn into(self) -> String {
		self.inner.to_string()
	}
}

impl TryFrom<String> for StorageID {
	type Error = StorageError;

	fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
		Ok(Self { inner: Ulid::from_string(&value).map_err(StorageError::ID)? })
	}
}

/// A record of a state change.
#[derive(Clone, Debug)]
pub struct StateChangeRecord {
	pub identifier: StorageID,
	pub data: StateChange,
}

/// A record of an event.
#[derive(Clone, Debug)]
pub struct EventRecord {
	pub identifier: StorageID,
	pub state_change_identifier: StorageID,
	pub data: Event,
	pub timestamp: NaiveDateTime,
}

/// A record of a snaoshot.
#[derive(Debug, Clone)]
pub struct SnapshotRecord {
	pub identifier: StorageID,
	pub statechange_qty: u32,
	pub state_change_identifier: StorageID,
	pub data: ChainState,
}
