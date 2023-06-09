#![warn(clippy::missing_docs_in_private_items)]

use std::{
	convert::TryInto,
	sync::Mutex,
};

pub use chrono::NaiveDateTime;
use chrono::Utc;
use raiden_primitives::types::{
	Address,
	BalanceHash,
	CanonicalIdentifier,
	Locksroot,
	TokenNetworkAddress,
};
use rusqlite::{
	params,
	Connection,
	ToSql,
};
use ulid::Ulid;

use self::types::{
	EventRecord,
	Result,
	SnapshotRecord,
	StateChangeRecord,
	StorageError,
	StorageID,
};
use crate::types::{
	ChainState,
	Event,
	StateChange,
};

/// Sqlite constants.
mod sqlite;
pub mod types;

/// The number of blocks before taking a snot of the chain state.
pub const SNAPSHOT_STATE_CHANGE_COUNT: u16 = 500;

/// Storage interface for the chain state.
pub struct StateStorage {
	/// The rusqlite connection
	conn: Mutex<Connection>,
}

impl StateStorage {
	/// Create an instance of `StateStorage`.
	pub fn new(conn: Connection) -> Self {
		Self { conn: Mutex::new(conn) }
	}

	/// Create tables if not already created.
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

	/// Store chain state snapshot.
	pub fn store_snapshot(
		&self,
		state: ChainState,
		state_change_id: Option<StorageID>,
	) -> Result<()> {
		let serialized_state =
			serde_json::to_string(&state).map_err(StorageError::SerializationError)?;
		let sql = "
            INSERT INTO state_snapshot(identifier, statechange_id, statechange_qty, data)
            VALUES(?1, ?2, ?3, ?4)"
			.to_owned();
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
			.map_err(StorageError::Sql)?;

		Ok(())
	}

	/// Return all state changes.
	pub fn state_changes(&self) -> Result<Vec<StateChangeRecord>> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn
			.prepare("SELECT identifier, data FROM state_changes")
			.map_err(StorageError::Sql)?;

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

	/// Store a state change.
	pub fn store_state_change(&self, state_change: StateChange) -> Result<StorageID> {
		let serialized_state_change =
			serde_json::to_string(&state_change).map_err(StorageError::SerializationError)?;
		let sql = "INSERT INTO state_changes(identifier, data) VALUES(?1, ?2)".to_owned();
		let ulid = Ulid::new();
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute(&sql, params![&ulid.to_string(), serialized_state_change])
			.map_err(StorageError::Sql)?;
		Ok(ulid.into())
	}

	/// Store a list of events.
	pub fn store_events(&self, state_change_id: StorageID, events: Vec<Event>) -> Result<()> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;

		for event in events {
			let serialized_event =
				serde_json::to_string(&event).map_err(StorageError::SerializationError)?;
			let sql =
                "INSERT INTO state_events(identifier, source_statechange_id, data, timestamp) VALUES(?1, ?2, ?3, ?4)".to_owned();
			conn.execute(
				&sql,
				params![
					&Ulid::new().to_string(),
					&state_change_id.to_string(),
					serialized_event,
					Utc::now().naive_local()
				],
			)
			.map_err(StorageError::Sql)?;
		}
		Ok(())
	}

	/// Get the last snapshot prior to a specific state change identifier.
	pub fn get_snapshot_before_state_change(
		&self,
		state_change_id: StorageID,
	) -> Result<SnapshotRecord> {
		let sql = "SELECT identifier, statechange_qty, statechange_id, data
			FROM state_snapshot
			WHERE statechange_id <= ?1 or statechange_id IS NULL
			ORDER BY identifier DESC
			LIMIT 1"
			.to_owned();
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn.prepare(&sql).map_err(StorageError::Sql)?;
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

	/// Get the list of state changes in range of ULIDs.
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

	/// Get a state change based on data field attributes.
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

	/// Get a state change that contains a balance proof that matches the provided `balance_hash`.
	pub fn get_state_change_with_balance_proof_by_balance_hash(
		&self,
		canonical_identifier: CanonicalIdentifier,
		balance_hash: BalanceHash,
		recipient: Address,
	) -> Result<Option<StateChangeRecord>> {
		let criteria = vec![
			(
				"balance_proof.canonical_identifier.chain_identifier".to_owned(),
				canonical_identifier.chain_identifier.to_string(),
			),
			(
				"balance_proof.canonical_identifier.token_network_address".to_owned(),
				format!("0x{}", hex::encode(canonical_identifier.token_network_address)),
			),
			(
				"balance_proof.canonical_identifier.channel_identifier".to_owned(),
				canonical_identifier.channel_identifier.to_string(),
			),
			("balance_hash".to_owned(), format!("0x{}", hex::encode(balance_hash))),
			("recipient".to_owned(), format!("0x{}", hex::encode(recipient))),
		];

		let mut where_cond = "".to_owned();
		let mut query_values: Vec<&dyn ToSql> = vec![];
		let mut it = criteria.iter().enumerate().peekable();
		while let Some((i, (field, value))) = it.next() {
			where_cond.push_str(&format!("json_extract(data, '$.{}')=?{}", field, i + 1));
			query_values.push(value);
			if it.peek().is_some() {
				where_cond.push_str(" AND ");
			}
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

		let mut rows = stmt.query(query_values.as_slice()).map_err(StorageError::Sql)?;

		let row = match rows.next().map_err(StorageError::Sql)? {
			Some(row) => row,
			None => return Err(StorageError::Other("State change not found")),
		};

		let identifier: StorageID =
			row.get::<usize, String>(0).map_err(StorageError::Sql)?.try_into()?;

		let data: String = row.get(1).map_err(StorageError::Sql)?;

		Ok(Some(StateChangeRecord {
			identifier,
			data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
		}))
	}

	/// Get a state change that contains a balance proof that matches the provided `locksroot`.
	pub fn get_state_change_with_balance_proof_by_locksroot(
		&self,
		canonical_identifier: CanonicalIdentifier,
		locksroot: Locksroot,
		recipient: Address,
	) -> Result<Option<StateChangeRecord>> {
		let criteria = vec![
			(
				"balance_proof.canonical_identifier.chain_identifier".to_owned(),
				canonical_identifier.chain_identifier.to_string(),
			),
			(
				"balance_proof.canonical_identifier.token_network_address".to_owned(),
				format!("0x{}", hex::encode(canonical_identifier.token_network_address)),
			),
			(
				"balance_proof.canonical_identifier.channel_identifier".to_owned(),
				canonical_identifier.channel_identifier.to_string(),
			),
			("balance_proof.locksroot".to_owned(), format!("0x{}", hex::encode(locksroot))),
			("balance_proof.sender".to_owned(), format!("0x{}", hex::encode(recipient))),
		];

		let mut where_cond = "".to_owned();
		let mut query_values: Vec<&dyn ToSql> = vec![];
		let mut it = criteria.iter().enumerate().peekable();
		while let Some((i, (field, value))) = it.next() {
			where_cond.push_str(&format!("json_extract(data, '$.{}')=?{}", field, i + 1));
			query_values.push(value);
			if it.peek().is_some() {
				where_cond.push_str(" AND ");
			}
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

		let mut rows = stmt.query(query_values.as_slice()).map_err(StorageError::Sql)?;

		let row = match rows.next().map_err(StorageError::Sql)? {
			Some(row) => row,
			None => return Err(StorageError::Other("State change not found")),
		};

		let identifier: StorageID =
			row.get::<usize, String>(0).map_err(StorageError::Sql)?.try_into()?;

		let data: String = row.get(1).map_err(StorageError::Sql)?;

		Ok(Some(StateChangeRecord {
			identifier,
			data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
		}))
	}

	/// Get the latest event filtered by criteria of data field attributes.
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
			timestamp: Utc::now().naive_local(),
		}))
	}

	/// Get an event with a balance proof filtered by the `balance_hash`.
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
					format!("0x{}", hex::encode(canonical_identifier.token_network_address)),
				),
				(
					"balance_proof.canonical_identifier.channel_identifier".to_owned(),
					canonical_identifier.channel_identifier.to_string(),
				),
				("balance_hash".to_owned(), format!("0x{}", hex::encode(balance_hash))),
				("recipient".to_owned(), format!("0x{}", hex::encode(recipient))),
			],
			vec![
				(
					"transfer.balance_proof.canonical_identifier.chain_identifier".to_owned(),
					canonical_identifier.chain_identifier.to_string(),
				),
				(
					"transfer.balance_proof.canonical_identifier.token_network_address".to_owned(),
					format!("0x{}", hex::encode(canonical_identifier.token_network_address)),
				),
				(
					"transfer.balance_proof.canonical_identifier.channel_identifier".to_owned(),
					canonical_identifier.channel_identifier.to_string(),
				),
				("transfer.balance_hash".to_owned(), format!("0x{}", hex::encode(balance_hash))),
				("recipient".to_owned(), format!("0x{}", hex::encode(recipient))),
			],
		];

		let mut where_cond = "".to_owned();
		let mut query_values: Vec<&dyn ToSql> = vec![];
		let mut group_it = criteria.iter().peekable();
		while let Some(group) = group_it.next() {
			where_cond.push('(');
			let mut it = group.iter().enumerate().peekable();
			while let Some((i, (field, value))) = it.next() {
				where_cond.push_str(&format!("json_extract(data, '$.{}')=?{}", field, i + 1));
				query_values.push(value);
				if it.peek().is_some() {
					where_cond.push_str(" AND ");
				}
			}
			where_cond.push(')');
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
			timestamp: Utc::now().naive_local(),
		}))
	}

	/// Get an event with a balance proof filtered by the `locksroot`.
	pub fn get_event_with_balance_proof_by_locksroot(
		&self,
		canonical_identifier: CanonicalIdentifier,
		locksroot: Locksroot,
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
					format!("0x{}", hex::encode(canonical_identifier.token_network_address)),
				),
				(
					"balance_proof.canonical_identifier.channel_identifier".to_owned(),
					canonical_identifier.channel_identifier.to_string(),
				),
				("locksroot".to_owned(), format!("0x{}", hex::encode(locksroot))),
				("recipient".to_owned(), format!("0x{}", hex::encode(recipient))),
			],
			vec![
				(
					"transfer.balance_proof.canonical_identifier.chain_identifier".to_owned(),
					canonical_identifier.chain_identifier.to_string(),
				),
				(
					"transfer.balance_proof.canonical_identifier.token_network_address".to_owned(),
					format!("0x{}", hex::encode(canonical_identifier.token_network_address)),
				),
				(
					"transfer.balance_proof.canonical_identifier.channel_identifier".to_owned(),
					canonical_identifier.channel_identifier.to_string(),
				),
				("transfer.locksroot".to_owned(), format!("0x{}", hex::encode(locksroot))),
				("recipient".to_owned(), format!("0x{}", hex::encode(recipient))),
			],
		];

		let mut where_cond = "".to_owned();
		let mut query_values: Vec<&dyn ToSql> = vec![];
		let mut group_it = criteria.iter().peekable();
		while let Some(group) = group_it.next() {
			where_cond.push('(');
			let mut it = group.iter().enumerate().peekable();
			while let Some((i, (field, value))) = it.next() {
				where_cond.push_str(&format!("json_extract(data, '$.{}')=?{}", field, i + 1));
				query_values.push(value);
				if it.peek().is_some() {
					where_cond.push_str(" AND ");
				}
			}
			where_cond.push(')');
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
			timestamp: Utc::now().naive_local(),
		}))
	}

	/// Return events with timestamps.
	pub fn get_events_with_timestamps(&self) -> Result<Vec<EventRecord>> {
		let query = "
            SELECT
                identifier, data, source_statechange_id, timestamp
            FROM
                state_events
            ORDER BY identifier ASC
        ";

		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn.prepare(query).map_err(StorageError::Sql)?;
		let mut rows = stmt.query(params![]).map_err(StorageError::Sql)?;

		let mut events = vec![];

		while let Ok(Some(row)) = rows.next() {
			let identifier: String = row.get(0).map_err(StorageError::Sql)?;
			let data: String = row.get(1).map_err(StorageError::Sql)?;
			let state_change_identifier: StorageID =
				row.get::<usize, String>(2).map_err(StorageError::Sql)?.try_into()?;
			let timestamp: NaiveDateTime = row.get(3).map_err(StorageError::Sql)?;

			events.push(EventRecord {
				identifier: identifier.try_into()?,
				data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
				state_change_identifier,
				timestamp,
			})
		}

		Ok(events)
	}

	/// Get events of payments with timestamps attached.
	pub fn get_events_payment_history_with_timestamps(
		&self,
		token_network_address: Option<TokenNetworkAddress>,
		partner_address: Option<Address>,
	) -> Result<Vec<EventRecord>> {
		let token_network_address = token_network_address.map(|a| a.to_string());
		let partner_address = partner_address.map(|a| a.to_string());

		let mut params: Vec<&dyn ToSql> = vec![];

		let binding = (token_network_address, partner_address);
		let query = match binding {
			(Some(ref token_network_address), Some(ref partner_address)) => {
				let query = "
                        SELECT
                            identifier, data, source_statechange_id, timestamp
                        FROM
                            state_events
                        WHERE
                            json_extract(data, '$.type') IN ('PaymentReceivedSuccess', 'PaymentSentFailed', 'PaymentSentSuccess')
                        AND
                            json_extract(data, '$.token_network_address') LIKE ?1
                        AND
                            (
                                json_extract(data, '$.target') LIKE ?2
                                OR
                                json_extract(data, '$.initiator') LIKE ?2
                            )
                        ORDER BY identifier ASC";

				params.push(token_network_address);
				params.push(partner_address);
				query
			},
			(Some(ref token_network_address), None) => {
				let query = "
                        SELECT
                            identifier, data, source_statechange_id, timestamp
                        FROM
                            state_events
                        WHERE
                            json_extract(data, '$.type') IN ('PaymentReceivedSuccess', 'PaymentSentFailed', 'PaymentSentSuccess')
                        AND
                            json_extract(data, '$.token_network_address') LIKE ?1
                        ORDER BY identifier ASC
                        ";
				params.push(token_network_address);
				query
			},
			(None, Some(ref partner_address)) => {
				let query = "
                        SELECT
                            identifier, data, source_statechange_id, timestamp
                        FROM
                            state_events
                        WHERE
                            json_extract(data, '$.type') IN ('PaymentReceivedSuccess', 'PaymentSentFailed', 'PaymentSentSuccess')
                        AND
                            (
                            json_extract(data, '$.target') LIKE ?1
                            OR
                            json_extract(data, '$.initiator') LIKE ?1
                            )
                        ORDER BY identifier ASC
                        ";
				params.push(partner_address);
				query
			},
			(None, None) => {
				"
                    SELECT
                        identifier, data, source_statechange_id, timestamp
                    FROM
                        state_events
                    WHERE
                        json_extract(data, '$.type') IN ('PaymentReceivedSuccess', 'PaymentSentFailed', 'PaymentSentSuccess')
                    ORDER BY identifier ASC
                "
			},
		};

		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn.prepare(query).map_err(StorageError::Sql)?;

		let mut rows = stmt.query(params.as_slice()).map_err(StorageError::Sql)?;

		let mut events = vec![];

		while let Ok(Some(row)) = rows.next() {
			let identifier: String = row.get(0).map_err(StorageError::Sql)?;
			let data: String = row.get(1).map_err(StorageError::Sql)?;
			let state_change_identifier: StorageID =
				row.get::<usize, String>(2).map_err(StorageError::Sql)?.try_into()?;
			let timestamp: NaiveDateTime = row.get(3).map_err(StorageError::Sql)?;
			events.push(EventRecord {
				identifier: identifier.try_into()?,
				data: serde_json::from_str(&data).map_err(StorageError::SerializationError)?,
				state_change_identifier,
				timestamp,
			})
		}

		Ok(events)
	}
}
