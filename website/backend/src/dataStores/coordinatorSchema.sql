-- Coordinator Database Schema
-- Core run tracking
CREATE TABLE runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    program_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    run_index INTEGER NOT NULL,
    pubkey TEXT NOT NULL,
    created_at_slot INTEGER NOT NULL,
    created_at_time INTEGER NOT NULL,
    destroyed_at_slot INTEGER,
    destroyed_at_time INTEGER,
    last_updated_slot INTEGER NOT NULL,
    last_updated_time INTEGER NOT NULL,
    last_state_json TEXT, -- Serialized PsycheCoordinator
    UNIQUE(program_id, run_id, run_index)
);

-- Configuration changes over time
CREATE TABLE config_changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER REFERENCES runs(id),
    timestamp_slot INTEGER NOT NULL,
    timestamp_time INTEGER NOT NULL,
    model_json TEXT NOT NULL,
    config_json TEXT NOT NULL,
    metadata_json TEXT NOT NULL
);

-- Training steps tracking
CREATE TABLE training_steps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER REFERENCES runs(id),
    started_at_slot INTEGER NOT NULL,
    started_at_time INTEGER NOT NULL,
    ended_at_slot INTEGER,
    ended_at_time INTEGER,
    tokens_completed_at_start INTEGER NOT NULL
);

-- Pause/unpause events
CREATE TABLE pause_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER REFERENCES runs(id),
    event_type TEXT CHECK(event_type IN ('paused', 'unpaused')) NOT NULL,
    timestamp_slot INTEGER NOT NULL,
    timestamp_time INTEGER NOT NULL
);

-- Witness updates
CREATE TABLE witness_updates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER REFERENCES runs(id),
    witness_json TEXT NOT NULL,
    timestamp_slot INTEGER NOT NULL,
    timestamp_time INTEGER NOT NULL,
    step INTEGER,
    tokens_per_sec REAL,
    bandwidth_per_sec REAL,
    loss REAL,
    efficiency REAL,
    evals TEXT
);

-- Learning rate observations
CREATE TABLE lr_observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER REFERENCES runs(id),
    step INTEGER NOT NULL,
    learning_rate REAL NOT NULL
);

-- Transaction tracking
CREATE TABLE transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER REFERENCES runs(id),
    tx_hash TEXT NOT NULL,
    user_pubkey TEXT NOT NULL,
    method TEXT NOT NULL,
    data TEXT NOT NULL,
    timestamp_slot INTEGER NOT NULL,
    timestamp_time INTEGER NOT NULL
);

-- Metadata for sync state
CREATE TABLE sync_metadata (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    program_id TEXT NOT NULL,
    last_update_time INTEGER NOT NULL,
    highest_signature TEXT,
    highest_slot INTEGER
);

-- Indexes for performance
CREATE INDEX idx_runs_program_run ON runs(program_id, run_id);
CREATE INDEX idx_runs_pubkey ON runs(pubkey);
CREATE INDEX idx_config_changes_run ON config_changes(run_id);
CREATE INDEX idx_training_steps_run ON training_steps(run_id);
CREATE INDEX idx_pause_events_run ON pause_events(run_id);
CREATE INDEX idx_witness_updates_run ON witness_updates(run_id);
CREATE INDEX idx_lr_observations_run ON lr_observations(run_id);
CREATE INDEX idx_transactions_run ON transactions(run_id);
CREATE INDEX idx_transactions_timestamp ON transactions(timestamp_slot DESC);
CREATE INDEX idx_lr_observations_step ON lr_observations(run_id, step);
CREATE INDEX idx_witness_updates_timestamp ON witness_updates(timestamp_slot DESC);
