use std::convert::TryFrom;

use ulid::Ulid;

use crate::errors::TypeError;
#[derive(Clone, Copy, Debug)]
pub struct StateChangeID {
	inner: Ulid,
}

impl std::fmt::Display for StateChangeID {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.inner.to_string())
	}
}

impl From<Ulid> for StateChangeID {
	fn from(id: Ulid) -> Self {
		return Self { inner: id }
	}
}

impl Into<String> for StateChangeID {
	fn into(self) -> String {
		self.inner.to_string()
	}
}

impl TryFrom<String> for StateChangeID {
	type Error = TypeError;

	fn try_from(value: String) -> Result<Self, Self::Error> {
		Ok(Self {
			inner: Ulid::from_string(&value).map_err(|e| TypeError { msg: format!("{}", e) })?,
		})
	}
}
