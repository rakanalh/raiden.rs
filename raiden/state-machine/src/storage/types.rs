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

pub type Result<T> = std::result::Result<T, StorageError>;

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

#[derive(Clone, Copy, Debug)]
pub struct StorageID {
	pub(crate) inner: Ulid,
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

pub struct StateChangeRecord {
	pub identifier: StorageID,
	pub data: StateChange,
}

#[derive(Debug)]
pub struct EventRecord {
	pub identifier: StorageID,
	pub state_change_identifier: StorageID,
	pub data: Event,
	pub timestamp: NaiveDateTime,
}

pub struct SnapshotRecord {
	pub identifier: StorageID,
	pub statechange_qty: u32,
	pub state_change_identifier: StorageID,
	pub data: ChainState,
}
