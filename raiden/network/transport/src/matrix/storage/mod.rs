use std::sync::Mutex;

use derive_more::Display;
use rusqlite::{
	params,
	Connection,
	Error,
};

mod sqlite;

/// Result type for calling storage.
pub type Result<T> = std::result::Result<T, StorageError>;

/// The storage error type.
#[derive(Display, Debug)]
pub enum StorageError {
	#[display(fmt = "Storage lock poisoned")]
	CannotLock,
	#[display(fmt = "SQL Error: {}", _0)]
	Sql(rusqlite::Error),
}

/// Storage for the matrix transport layer.
pub struct MatrixStorage {
	conn: Mutex<Connection>,
}

impl MatrixStorage {
	/// Create a new instance of `MatrixStorage`
	pub fn new(conn: Connection) -> Self {
		Self { conn: Mutex::new(conn) }
	}

	/// Initialize storage and create tables.
	pub fn setup_database(&self) -> Result<()> {
		let setup_db_sql = format!(
			"
			PRAGMA foreign_keys=off;
			BEGIN TRANSACTION;
			{}{}
			COMMIT;
			PRAGMA foreign_keys=on;
			",
			sqlite::DB_CREATE_MATRIX_CONFIG,
			sqlite::DB_CREATE_MATRIX_MESSAGES,
		);
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute_batch(&setup_db_sql)
			.map_err(StorageError::Sql)?;

		Ok(())
	}

	/// Retrieve the last known sync token.
	pub fn get_sync_token(&self) -> Result<String> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn
			.prepare("SELECT sync_token FROM matrix_config")
			.map_err(StorageError::Sql)?;

		let sync_token: String = stmt.query_row([], |r| r.get(0)).map_err(StorageError::Sql)?;
		Ok(sync_token)
	}

	/// Set the last received sync token.
	pub fn set_sync_token(&self, sync_token: String) -> Result<()> {
		let sql = if self.get_sync_token().is_err() {
			"INSERT INTO matrix_config(sync_token) VALUES(?1)".to_string()
		} else {
			"UPDATE matrix_config SET sync_token=?1".to_string()
		};
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute(&sql, params![sync_token])
			.map_err(StorageError::Sql)?;
		Ok(())
	}

	/// Retrieve the list of queued messages.
	pub fn get_messages(&self) -> Result<String> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt =
			conn.prepare("SELECT data FROM matrix_messages").map_err(StorageError::Sql)?;

		let messages: String = match stmt.query_row([], |r| r.get(0)) {
			Ok(messages) => messages,
			Err(e) =>
				if let Error::QueryReturnedNoRows = e {
					return Ok(String::new())
				} else {
					return Err(StorageError::Sql(e))
				},
		};
		Ok(messages)
	}

	/// Storage queued messages.
	pub fn store_messages(&self, messages: String) -> Result<()> {
		let sql = if self.get_messages().is_err() {
			"INSERT INTO matrix_messages(data) VALUES(?1)".to_string()
		} else {
			"UPDATE matrix_messages SET data=?1".to_string()
		};
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute(&sql, params![messages])
			.map_err(StorageError::Sql)?;
		Ok(())
	}
}
