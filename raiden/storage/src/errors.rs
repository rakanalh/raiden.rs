use derive_more::Display;
use ulid::DecodeError;

#[derive(Display, Debug)]
pub enum StorageError {
	#[display(fmt = "Storage lock poisoned")]
	CannotLock,
	#[display(fmt = "Field unknown {}", _0)]
	FieldUnknown(rusqlite::Error),
	#[display(fmt = "Cannot serialize for storage {}", _0)]
	SerializationError(serde_json::Error),
	#[display(fmt = "SQL Error: {}", _0)]
	Sql(rusqlite::Error),
	#[display(fmt = "Cannot map item to for storage: {}", _0)]
	Cast(rusqlite::Error),
	#[display(fmt = "Cannot convert value to Ulid: {}", _0)]
	ID(DecodeError),
	#[display(fmt = "Error: {}", _0)]
	Other(&'static str),
}
