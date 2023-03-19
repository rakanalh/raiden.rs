use std::convert::TryFrom;

use ulid::Ulid;

use crate::errors::StorageError;

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

	fn try_from(value: String) -> Result<Self, Self::Error> {
		Ok(Self { inner: Ulid::from_string(&value).map_err(StorageError::ID)? })
	}
}
