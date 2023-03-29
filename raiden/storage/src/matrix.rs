use std::sync::Mutex;

use rusqlite::{
	params,
	Connection,
};

use crate::{
	errors::StorageError,
	sqlite,
	types::Result,
};

pub struct MatrixStorage {
	conn: Mutex<Connection>,
}

impl MatrixStorage {
	pub fn new(conn: Connection) -> Self {
		Self { conn: Mutex::new(conn) }
	}

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

	pub fn get_sync_token(&self) -> Result<String> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn
			.prepare("SELECT sync_token FROM matrix_config")
			.map_err(|e| StorageError::Sql(e))?;

		let sync_token: String = stmt.query_row([], |r| r.get(0)).map_err(StorageError::Sql)?;
		Ok(sync_token)
	}

	pub fn set_sync_token(&self, sync_token: String) -> Result<()> {
		let sql = format!("UPDATE matrix_config SET sync_token=?1");
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute(&sql, params![sync_token])
			.map_err(|e| StorageError::Sql(e))?;
		Ok(())
	}

	pub fn get_messages(&self) -> Result<String> {
		let conn = self.conn.lock().map_err(|_| StorageError::CannotLock)?;
		let mut stmt = conn
			.prepare("SELECT data FROM matrix_messages")
			.map_err(|e| StorageError::Sql(e))?;

		let messages: String = stmt.query_row([], |r| r.get(0)).map_err(StorageError::Sql)?;
		Ok(messages)
	}

	pub fn store_messages(&self, messages: String) -> Result<()> {
		let sql = format!("UPDATE matrix_messages SET data=?1");
		self.conn
			.lock()
			.map_err(|_| StorageError::CannotLock)?
			.execute(&sql, params![messages])
			.map_err(|e| StorageError::Sql(e))?;
		Ok(())
	}
}
