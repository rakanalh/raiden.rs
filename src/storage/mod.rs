use derive_more::Display;
use rusqlite::Connection;
use rusqlite::{
    params,
    NO_PARAMS,
};
use std::convert::TryInto;
use ulid::Ulid;

use self::types::StateChangeID;
use crate::{
    errors::{
        RaidenError,
        TypeError,
    },
    state_machine::types::{
        Event,
        StateChange,
    },
};

mod sqlite;
mod types;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Display, Debug)]
pub enum StorageError {
    #[display(fmt = "Field unknown {}", _0)]
    FieldUnknown(rusqlite::Error),
    #[display(fmt = "Cannot serialize for storage")]
    SerializationError,
    #[display(fmt = "SQL Error: {}", _0)]
    Sql(rusqlite::Error),
    #[display(fmt = "Cannot map item to for storage: {}", _0)]
    Cast(rusqlite::Error),
    #[display(fmt = "Cannot convert value to Ulid: {}", _0)]
    ID(TypeError),
    #[display(fmt = "Error: {}", _0)]
    Other(&'static str),
}

impl From<StorageError> for RaidenError {
    fn from(e: StorageError) -> Self {
        RaidenError { msg: format!("{}", e) }
    }
}

pub struct StateChangeRecord {
    pub identifier: StateChangeID,
    pub data: String,
}

pub struct SnapshotRecord {
    pub identifier: StateChangeID,
    pub statechange_qty: u32,
    pub state_change_identifier: StateChangeID,
    pub data: String,
}

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn setup_database(&self) -> Result<()> {
        let setup_db_sql = format!(
            "
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
        self.conn.execute_batch(&setup_db_sql).map_err(StorageError::Sql)?;

        Ok(())
    }

    pub fn state_changes(&self) -> Result<Vec<StateChangeRecord>> {
        let mut stmt = self.conn
            .prepare("SELECT identifier, data FROM state_changes")
            .map_err(|e| StorageError::Sql(e))?;

        let mut rows = stmt.query(NO_PARAMS).map_err(StorageError::Sql)?;

        let mut state_changes = vec![];

        while let Ok(Some(row)) = rows.next() {
            let identifier: String = row.get(0).map_err(StorageError::Sql)?;
            state_changes.push(StateChangeRecord {
                identifier: identifier.try_into().map_err(StorageError::ID)?,
                data: row.get(1).map_err(StorageError::Sql)?,
            })
        }

        Ok(state_changes)
    }

    pub fn store_state_change(&self, state_change: StateChange) -> Result<Ulid> {
        let serialized_state_change =
            serde_json::to_string(&state_change).map_err(|_| StorageError::SerializationError)?;
        let sql = format!("INSERT INTO state_changes(identifier, data) VALUES(?1, ?2)");
        let ulid = Ulid::new();
        self.conn.execute(&sql, params![&ulid.to_string(), serialized_state_change])
            .map_err(|e| StorageError::Sql(e))?;
        Ok(ulid)
    }

    pub fn store_events(&self, state_change_id: Ulid, events: Vec<Event>) -> Result<()> {
        let serialized_events = serde_json::to_string(&events).map_err(|_| StorageError::SerializationError)?;
        let sql = format!("INSERT INTO state_events(identifier, source_statechange_id, data) VALUES(?1, ?2, ?3)");
        self.conn.execute(
            &sql,
            params![
                &Ulid::new().to_string(),
                &state_change_id.to_string(),
                serialized_events
            ],
        )
        .map_err(|e| StorageError::Sql(e))?;
        Ok(())
    }

    pub fn get_snapshot_before_state_change(&self, state_change_id: Ulid) -> Result<SnapshotRecord> {
        let sql = format!(
            "SELECT identifier, statechange_qty, statechange_id, data
			FROM state_snapshot
			WHERE statechange_id <= ?1 or statechange_id IS NULL
			ORDER BY identifier DESC
			LIMIT 1"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| StorageError::Sql(e))?;
        let mut rows = stmt
            .query(params![state_change_id.to_string()])
            .map_err(StorageError::Sql)?;
        let row = match rows.next().map_err(StorageError::Sql)? {
            Some(row) => row,
            None => return Err(StorageError::Other("Many snapshots found")),
        };

        let identifier: String = row.get(0).map_err(StorageError::Sql)?;
        let state_change_identifier: String = row.get(2).map_err(StorageError::Sql)?;
        Ok(SnapshotRecord {
            identifier: identifier.try_into().map_err(StorageError::ID)?,
            statechange_qty: row.get(1).map_err(StorageError::Sql)?,
            state_change_identifier: state_change_identifier.try_into().map_err(StorageError::ID)?,
            data: row.get(3).map_err(StorageError::Sql)?,
        })
    }

    pub fn get_state_changes_in_range(
        &self,
        start_state_change: StateChangeID,
        end_state_change: StateChangeID,
    ) -> Result<Vec<StateChangeRecord>> {
        let mut stmt = self.conn
            .prepare(
                "SELECT identifier, data FROM state_changes
			WHERE identifier BETWEEEN ?1 AND ?2",
            )
            .map_err(|e| StorageError::Sql(e))?;

        let start_state_change: String = start_state_change.into();
        let end_state_change: String = end_state_change.into();

        let mut rows = stmt
            .query(params![start_state_change, end_state_change,])
            .map_err(StorageError::Sql)?;

        let mut state_changes = vec![];

        while let Ok(Some(row)) = rows.next() {
            let identifier: String = row.get(0).map_err(StorageError::Sql)?;
            state_changes.push(StateChangeRecord {
                identifier: identifier.try_into().map_err(StorageError::ID)?,
                data: row.get(1).map_err(StorageError::Sql)?,
            })
        }

        Ok(state_changes)
    }
}
