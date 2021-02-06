use derive_more::Display;
use rusqlite::{NO_PARAMS, params};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use ulid::{DecodeError, Ulid};

use crate::{enums::{Event, StateChange}, errors::RaidenError};

mod sqlite;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Display, Debug)]
pub enum StorageError {
	#[display(fmt = "Storage lock poisoned")]
	CannotLock,
	#[display(fmt = "Field unknown {}", _0)]
	FieldUnknown(rusqlite::Error),
	#[display(fmt = "Cannot serialize for storage")]
	SerializationError,
	#[display(fmt = "SQL Error: {}", _0)]
	Sql(rusqlite::Error),
	#[display(fmt = "Cannot map item to for storage: {}", _0)]
	Cast(rusqlite::Error),
	#[display(fmt = "Cannot convert value to Ulid: {}", _0)]
	Ulid(DecodeError),
	#[display(fmt = "Error: {}", _0)]
	Other(&'static str),
}

impl From<StorageError> for RaidenError {
    fn from(e: StorageError) -> Self {
        RaidenError {
			msg: format!("{}", e)
		}
    }
}

pub struct StateChangeRecord {
	pub identifier: Ulid,
	pub data: String,
}

pub struct SnapshotRecord {
	pub identifier: Ulid,
	pub statechange_qty: u32,
	pub state_change_identifier: Ulid,
	pub data: String,
}

pub struct Storage {
	conn: Arc<Mutex<Connection>>,
}

impl Storage {
	pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
		Self {
			conn,
		}
	}

	pub fn setup_database(&self) -> Result<()> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let setup_db_sql = format!("
			PRAGMA foreign_keys=off;
			BEGIN TRANSACTION;
			{}{}{}{}{}
			COMMIT;
			PRAGMA foreign_keys=on;
			",
			sqlite::DB_CREATE_SETTINGS,
			sqlite::DB_CREATE_STATE_CHANGES,
			sqlite::DB_CREATE_SNAPSHOT,
			sqlite::DB_CREATE_STATE_EVENTS,
			sqlite::DB_CREATE_RUNS,
		);
		conn.execute_batch(&setup_db_sql).map_err(StorageError::Sql)?;

		Ok(())
	}

	pub fn state_changes(&self) -> Result<Vec<StateChangeRecord>> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;

		let mut stmt = conn.prepare("SELECT identifier, data FROM state_changes")
			.map_err(|e| StorageError::Sql(e))?;

		let mut rows = stmt.query(NO_PARAMS)
			.map_err(StorageError::Sql)?;

		let mut state_changes = vec![];

		while let Ok(Some(row)) = rows.next() {
			let identifier: String = row.get(0).map_err(StorageError::Sql)?;
			state_changes.push(StateChangeRecord {
				identifier: Ulid::from_string(&identifier).map_err(StorageError::Ulid)?,
				data: row.get(1).map_err(StorageError::Sql)?,
			})
		}

		Ok(state_changes)
	}

	pub fn store_state_change(
		&self,
		state_change: StateChange,
	) -> Result<Ulid> {
		let serialized_state_change = serde_json::to_string(&state_change)
			.map_err(|_| StorageError::SerializationError)?;
		let sql = format!("INSERT INTO state_changes(identifier, data) VALUES(?1, ?2)");
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let ulid = Ulid::new();
		conn.execute(&sql, params![&ulid.to_string(), serialized_state_change])
			.map_err(|e| StorageError::Sql(e))?;
		Ok(ulid)
	}


	pub fn store_events(
		&self,
		state_change_id: Ulid,
		events: Vec<Event>,
	) -> Result<()> {
		let serialized_events = serde_json::to_string(&events)
			.map_err(|_| StorageError::SerializationError)?;
		let sql = format!("INSERT INTO state_events(identifier, source_statechange_identifier, data) VALUES(?1, ?2, ?3)");
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		conn.execute(&sql, params![&Ulid::new().to_string(), &state_change_id.to_string(), serialized_events])
			.map_err(|e| StorageError::Sql(e))?;
		Ok(())
	}

	pub fn get_snapshot_before_state_change(&self, state_change_id: Ulid) -> Result<SnapshotRecord>  {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;

		let sql = format!("SELECT identifier, statechange_qty, statechange_id, data FROM state_snapshot WHERE statechange_id <= ?1 or statechange_id IS NULL ORDER BY identifier DESC LIMIT 1");
		let mut stmt = conn.prepare(&sql).map_err(|e| StorageError::Sql(e))?;
		let mut rows = stmt
			.query(params![state_change_id.to_string()])
			.map_err(StorageError::Sql)?;
		let row = match rows
			.next()
			.map_err(StorageError::Sql)? {
				Some(row) => row,
				None => return Err(StorageError::Other("Many snapshots found"))
			};

		let identifier: String = row.get(0).map_err(StorageError::Sql)?;
		let state_change_identifier: String = row.get(2).map_err(StorageError::Sql)?;
		Ok(SnapshotRecord {
			identifier: Ulid::from_string(&identifier).map_err(StorageError::Ulid)?,
			statechange_qty: row.get(1).map_err(StorageError::Sql)?,
			state_change_identifier: Ulid::from_string(&state_change_identifier).map_err(StorageError::Ulid)?,
			data: row.get(3).map_err(StorageError::Sql)?,

		})
	}
}
