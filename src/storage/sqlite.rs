
pub(super) const DB_CREATE_SETTINGS: &str = "
CREATE TABLE IF NOT EXISTS settings (
    name VARCHAR[24] UNIQUE PRIMARY KEY NOT NULL,
    value TEXT
);
";

pub(super) const DB_CREATE_STATE_CHANGES: &str = "
CREATE TABLE IF NOT EXISTS state_changes (
    identifier ULID PRIMARY KEY NOT NULL,
    data JSON,
    timestamp TIMESTAMP DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')) NOT NULL
);
";

pub(super) const DB_CREATE_SNAPSHOT: &str = "
CREATE TABLE IF NOT EXISTS state_snapshot (
    identifier ULID PRIMARY KEY NOT NULL,
    statechange_id ULID UNIQUE,
    statechange_qty INTEGER,
    data JSON,
    timestamp TIMESTAMP DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')) NOT NULL,
    FOREIGN KEY(statechange_id) REFERENCES state_changes(identifier)
);
";

pub(super) const DB_CREATE_STATE_EVENTS: &str = "
CREATE TABLE IF NOT EXISTS state_events (
    identifier ULID PRIMARY KEY NOT NULL,
    source_statechange_id ULID NOT NULL,
    data JSON,
    timestamp TIMESTAMP DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')) NOT NULL,
    FOREIGN KEY(source_statechange_id) REFERENCES state_changes(identifier)
);
";

pub(super) const DB_CREATE_RUNS: &str = "
CREATE TABLE IF NOT EXISTS runs (
    started_at TIMESTAMP DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')) PRIMARY KEY NOT NULL,
    raiden_version TEXT NOT NULL
);
";
