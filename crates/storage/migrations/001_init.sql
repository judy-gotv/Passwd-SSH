-- OpsDeck initial schema (docs §5).
-- Author: Passwd Word

CREATE TABLE IF NOT EXISTS hosts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    group_id TEXT,
    host TEXT NOT NULL,
    port INTEGER NOT NULL DEFAULT 22,
    username TEXT NOT NULL,
    auth_type TEXT NOT NULL,
    credential_id TEXT,
    -- Extension beyond docs §5.1: passphrase lives in its own credential.
    passphrase_credential_id TEXT,
    proxy_jump_id TEXT,
    startup_command TEXT,
    tags TEXT,
    favorite INTEGER NOT NULL DEFAULT 0,
    last_connected_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_hosts_group ON hosts(group_id);
CREATE INDEX IF NOT EXISTS idx_hosts_name ON hosts(name);

CREATE TABLE IF NOT EXISTS credentials (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS host_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS snippets (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    command TEXT NOT NULL,
    description TEXT,
    host_id TEXT,
    tags TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_snippets_title ON snippets(title);

CREATE TABLE IF NOT EXISTS transfer_tasks (
    id TEXT PRIMARY KEY,
    host_id TEXT NOT NULL,
    direction TEXT NOT NULL,
    local_path TEXT NOT NULL,
    remote_path TEXT NOT NULL,
    status TEXT NOT NULL,
    bytes_total INTEGER,
    bytes_done INTEGER DEFAULT 0,
    error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_transfers_host ON transfer_tasks(host_id);
CREATE INDEX IF NOT EXISTS idx_transfers_status ON transfer_tasks(status);

CREATE TABLE IF NOT EXISTS metric_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    host_id TEXT NOT NULL,
    collected_at INTEGER NOT NULL,
    cpu_percent REAL,
    memory_used_bytes INTEGER,
    memory_total_bytes INTEGER,
    disk_used_bytes INTEGER,
    disk_total_bytes INTEGER,
    load_1 REAL,
    load_5 REAL,
    load_15 REAL,
    network_rx_bytes INTEGER,
    network_tx_bytes INTEGER
);
CREATE INDEX IF NOT EXISTS idx_metrics_host_time ON metric_snapshots(host_id, collected_at);

-- Key/value store for app settings (theme, sync state, etc.).
CREATE TABLE IF NOT EXISTS kv_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
