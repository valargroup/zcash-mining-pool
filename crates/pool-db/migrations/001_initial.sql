CREATE TABLE IF NOT EXISTS miners (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    address TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS workers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    miner_id INTEGER NOT NULL REFERENCES miners(id),
    name TEXT NOT NULL,
    last_seen TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(miner_id, name)
);

CREATE TABLE IF NOT EXISTS shares (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    worker_id INTEGER NOT NULL REFERENCES workers(id),
    job_id TEXT NOT NULL,
    difficulty REAL NOT NULL,
    is_block INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS blocks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    height INTEGER NOT NULL,
    hash TEXT NOT NULL UNIQUE,
    reward INTEGER NOT NULL,  -- in zatoshis
    status TEXT NOT NULL DEFAULT 'pending', -- pending, confirmed, orphaned
    found_by INTEGER REFERENCES workers(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS balances (
    miner_id INTEGER PRIMARY KEY REFERENCES miners(id),
    pending INTEGER NOT NULL DEFAULT 0,   -- in zatoshis
    paid INTEGER NOT NULL DEFAULT 0       -- in zatoshis
);

CREATE TABLE IF NOT EXISTS payouts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    miner_id INTEGER NOT NULL REFERENCES miners(id),
    txid TEXT,
    amount INTEGER NOT NULL,  -- in zatoshis
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_shares_created_at ON shares(created_at);
CREATE INDEX IF NOT EXISTS idx_shares_worker_id ON shares(worker_id);
CREATE INDEX IF NOT EXISTS idx_blocks_status ON blocks(status);
CREATE INDEX IF NOT EXISTS idx_blocks_height ON blocks(height);
