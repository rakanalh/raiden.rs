use std::{
	convert::TryInto,
	sync::Mutex,
};

use raiden_primitives::types::{
	Address,
	BalanceHash,
	CanonicalIdentifier,
};
use raiden_state_machine::types::{
	ChainState,
	Event,
	StateChange,
};
use rusqlite::{
	params,
	Connection,
	ToSql,
};
use ulid::Ulid;

use crate::{
	errors::StorageError,
	sqlite,
	types::StorageID,
};

pub type Result<T> = std::result::Result<T, StorageError>;

pub struct StateChangeRecord {
	pub identifier: StorageID,
	pub data: StateChange,
}

pub struct EventRecord {
	pub identifier: StorageID,
	pub state_change_identifier: StorageID,
	pub data: Event,
}

pub struct SnapshotRecord {
	pub identifier: StorageID,
	pub statechange_qty: u32,
	pub state_change_identifier: StorageID,
	pub data: ChainState,
}

pub struct StateStorage {
	conn: Mutex<Connection>,
}

impl StateStorage {
	pub fn new(conn: Connection) -> Self {
		Self { conn: Mutex::new(conn) }
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
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute_batch(&setup_db_sql)
			.map_err(StorageError::Sql)?;

		Ok(())
	}

	pub fn store_snapshot(
		&self,
		state: ChainState,
		state_change_id: Option<StorageID>,
	) -> Result<()> {
		let serialized_state =
			serde_json::to_string(&state).map_err(StorageError::SerializationError)?;
		let sql = format!(
			"
            INSERT INTO state_snapshot(identifier, statechange_id, statechange_qty, data)
            VALUES(?1, ?2, ?3, ?4)"
		);
		let ulid = Ulid::new();
		let state_change_id = match state_change_id {
			Some(sc) => sc.inner,
			None => Ulid::nil(),
		};
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute(
				&sql,
				params![&ulid.to_string(), state_change_id.to_string(), 0, serialized_state,],
			)
			.map_err(|e| StorageError::Sql(e))?;

		Ok(())
	}

	pub fn state_changes(&self) -> Result<Vec<StateChangeRecord>> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn
			.prepare("SELECT identifier, data FROM state_changes")
			.map_err(|e| StorageError::Sql(e))?;

		let mut rows = stmt.query([]).map_err(StorageError::Sql)?;

		let mut state_changes = vec![];

		while let Ok(Some(row)) = rows.next() {
			let identifier: String = row.get(0).map_err(StorageError::Sql)?;
			let data: String = row.get(1).map_err(StorageError::Sql)?;
			state_changes.push(StateChangeRecord {
				identifier: identifier.try_into()?,
				data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
			})
		}

		Ok(state_changes)
	}

	pub fn store_state_change(&self, state_change: StateChange) -> Result<StorageID> {
		let serialized_state_change =
			serde_json::to_string(&state_change).map_err(StorageError::SerializationError)?;
		let sql = format!("INSERT INTO state_changes(identifier, data) VALUES(?1, ?2)");
		let ulid = Ulid::new();
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute(&sql, params![&ulid.to_string(), serialized_state_change])
			.map_err(|e| StorageError::Sql(e))?;
		Ok(ulid.into())
	}

	pub fn store_events(&self, state_change_id: StorageID, events: Vec<Event>) -> Result<()> {
		let serialized_events =
			serde_json::to_string(&events).map_err(StorageError::SerializationError)?;
		let sql = format!(
			"INSERT INTO state_events(identifier, source_statechange_id, data) VALUES(?1, ?2, ?3)"
		);
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute(
				&sql,
				params![&Ulid::new().to_string(), &state_change_id.to_string(), serialized_events],
			)
			.map_err(|e| StorageError::Sql(e))?;
		Ok(())
	}

	pub fn get_snapshot_before_state_change(
		&self,
		state_change_id: StorageID,
	) -> Result<SnapshotRecord> {
		let sql = format!(
			"SELECT identifier, statechange_qty, statechange_id, data
			FROM state_snapshot
			WHERE statechange_id <= ?1 or statechange_id IS NULL
			ORDER BY identifier DESC
			LIMIT 1"
		);
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn.prepare(&sql).map_err(|e| StorageError::Sql(e))?;
		let mut rows =
			stmt.query(params![state_change_id.to_string()]).map_err(StorageError::Sql)?;
		let row = match rows.next().map_err(StorageError::Sql)? {
			Some(row) => row,
			None => return Err(StorageError::Other("Many snapshots found")),
		};

		let identifier: String = row.get(0).map_err(StorageError::Sql)?;
		let state_change_identifier: String = row.get(2).map_err(StorageError::Sql)?;
		let data: String = row.get(3).map_err(StorageError::Sql)?;
		Ok(SnapshotRecord {
			identifier: identifier.try_into()?,
			statechange_qty: row.get(1).map_err(StorageError::Sql)?,
			state_change_identifier: state_change_identifier.try_into()?,
			data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
		})
	}

	pub fn get_state_changes_in_range(
		&self,
		start_state_change: StorageID,
		end_state_change: StorageID,
	) -> Result<Vec<StateChangeRecord>> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn
			.prepare(
				"SELECT identifier, data FROM state_changes
                WHERE identifier>=?1 AND identifier<=?2",
			)
			.map_err(StorageError::Sql)?;

		let start_state_change: String = start_state_change.into();
		let end_state_change: String = end_state_change.into();

		let mut rows = stmt
			.query(params![start_state_change, end_state_change,])
			.map_err(StorageError::Sql)?;

		let mut state_changes = vec![];

		while let Ok(Some(row)) = rows.next() {
			let identifier: String = row.get(0).map_err(StorageError::Sql)?;
			let data: String = row.get(1).map_err(StorageError::Sql)?;
			state_changes.push(StateChangeRecord {
				identifier: identifier.try_into()?,
				data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
			})
		}

		Ok(state_changes)
	}

	pub fn get_latest_state_change_by_data_field(
		&self,
		criteria: Vec<(String, String)>,
	) -> Result<Option<StateChangeRecord>> {
		let mut where_cond = "".to_owned();
		for (i, (field, _)) in criteria.iter().enumerate() {
			where_cond.push_str(&format!("{}=?{}", field, i + 1));
		}
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn
			.prepare(&format!(
				"SELECT identifier, data FROM state_changes
                    WHERE {}
                    ORDER BY identifier DESC
                    LIMIT 1",
				where_cond
			))
			.map_err(StorageError::Sql)?;

		let query_values: Vec<_> = criteria.iter().map(|(_, v)| v as &dyn ToSql).collect();

		let mut rows = stmt.query(query_values.as_slice()).map_err(StorageError::Sql)?;

		let row = match rows.next().map_err(StorageError::Sql)? {
			Some(row) => row,
			None => return Err(StorageError::Other("State change not found")),
		};
		let identifier: String = row.get(0).map_err(StorageError::Sql)?;
		let data: String = row.get(1).map_err(StorageError::Sql)?;
		Ok(Some(StateChangeRecord {
			identifier: identifier.try_into()?,
			data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
		}))
	}

	pub fn get_latest_event_by_data_field(
		&self,
		criteria: Vec<(String, String)>,
	) -> Result<Option<EventRecord>> {
		let mut where_cond = "".to_owned();
		for (i, (field, _)) in criteria.iter().enumerate() {
			where_cond.push_str(&format!("{}=?{}", field, i + 1));
		}
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn
			.prepare(&format!(
				"SELECT identifier, source_statechange_id, data FROM state_events
                    WHERE {}
                    ORDER BY identifier DESC
                    LIMIT 1",
				where_cond
			))
			.map_err(StorageError::Sql)?;

		let query_values: Vec<_> = criteria.iter().map(|(_, v)| v as &dyn ToSql).collect();

		let mut rows = stmt.query(query_values.as_slice()).map_err(StorageError::Sql)?;

		let row = match rows.next().map_err(StorageError::Sql)? {
			Some(row) => row,
			None => return Err(StorageError::Other("Event not found")),
		};
		let identifier: StorageID =
			row.get::<usize, String>(0).map_err(StorageError::Sql)?.try_into()?;
		let state_change_identifier: StorageID =
			row.get::<usize, String>(1).map_err(StorageError::Sql)?.try_into()?;
		let data: String = row.get(2).map_err(StorageError::Sql)?;
		Ok(Some(EventRecord {
			identifier,
			state_change_identifier,
			data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
		}))
	}

	pub fn get_event_with_balance_proof_by_balance_hash(
		&self,
		canonical_identifier: CanonicalIdentifier,
		balance_hash: BalanceHash,
		recipient: Address,
	) -> Result<Option<EventRecord>> {
		let criteria = vec![
			vec![
				(
					"balance_proof.canonical_identifier.chain_identifier".to_owned(),
					canonical_identifier.chain_identifier.to_string(),
				),
				(
					"balance_proof.canonical_identifier.token_network_address".to_owned(),
					canonical_identifier.token_network_address.to_string(),
				),
				(
					"balance_proof.canonical_identifier.channel_identifier".to_owned(),
					canonical_identifier.channel_identifier.to_string(),
				),
				("balance_hash".to_owned(), balance_hash.to_string()),
				("recipient".to_owned(), recipient.to_string()),
			],
			vec![
				(
					"transfer.balance_proof.canonical_identifier.chain_identifier".to_owned(),
					canonical_identifier.chain_identifier.to_string(),
				),
				(
					"transfer.balance_proof.canonical_identifier.token_network_address".to_owned(),
					canonical_identifier.token_network_address.to_string(),
				),
				(
					"transfer.balance_proof.canonical_identifier.channel_identifier".to_owned(),
					canonical_identifier.channel_identifier.to_string(),
				),
				("transfer.balance_hash".to_owned(), balance_hash.to_string()),
				("recipient".to_owned(), recipient.to_string()),
			],
		];

		let mut where_cond = "".to_owned();
		let mut query_values: Vec<&dyn ToSql> = vec![];
		let mut group_it = criteria.iter().peekable();
		while let Some(group) = group_it.next() {
			where_cond.push_str("(");
			let mut it = group.iter().enumerate().peekable();
			while let Some((i, (field, value))) = it.next() {
				where_cond.push_str(&format!("{}=?{}", field, i + 1));
				query_values.push(value);
				if it.peek().is_some() {
					where_cond.push_str(" AND ");
				}
			}
			where_cond.push_str(")");
			if group_it.next().is_some() {
				where_cond.push_str(" OR ")
			}
		}

		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;

		let mut stmt = conn
			.prepare(&format!(
				"SELECT identifier, source_statechange_id, data FROM state_events
                    WHERE {}
                    ORDER BY identifier DESC
                    LIMIT 1",
				where_cond
			))
			.map_err(StorageError::Sql)?;

		let mut rows = stmt.query(query_values.as_slice()).map_err(StorageError::Sql)?;

		let row = match rows.next().map_err(StorageError::Sql)? {
			Some(row) => row,
			None => return Err(StorageError::Other("Event not found")),
		};

		let identifier: StorageID =
			row.get::<usize, String>(0).map_err(StorageError::Sql)?.try_into()?;

		let state_change_identifier: StorageID =
			row.get::<usize, String>(1).map_err(StorageError::Sql)?.try_into()?;

		let data: String = row.get(2).map_err(StorageError::Sql)?;

		Ok(Some(EventRecord {
			identifier,
			state_change_identifier,
			data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
		}))
	}
}
