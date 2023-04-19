pub(super) const DB_CREATE_MATRIX_CONFIG: &str = "
CREATE TABLE IF NOT EXISTS matrix_config (sync_token TEXT);
";

pub(super) const DB_CREATE_MATRIX_MESSAGES: &str = "
CREATE TABLE IF NOT EXISTS matrix_messages (data TEXT);
";
