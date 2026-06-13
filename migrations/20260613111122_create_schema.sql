CREATE TABLE IF NOT EXISTS models (
    model_id      TEXT PRIMARY KEY,
    vote_count    INTEGER NOT NULL DEFAULT 0,
    hf_link       TEXT,
    created_by_ip TEXT,
    created_at    DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS votes (
    client_uuid TEXT NOT NULL,
    model_id    TEXT NOT NULL,
    ip_address  TEXT NOT NULL,
    voted_at    DATETIME NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (client_uuid, model_id)
);

CREATE TABLE IF NOT EXISTS deliveries (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    model_id     TEXT NOT NULL,
    vote_count   INTEGER NOT NULL,
    hf_link      TEXT NOT NULL,
    notes        TEXT,
    delivered_at DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_votes_ip_time
    ON votes(ip_address, voted_at);

CREATE INDEX IF NOT EXISTS idx_models_ip_time
    ON models(created_by_ip, created_at);

CREATE INDEX IF NOT EXISTS idx_deliveries_time
    ON deliveries(delivered_at DESC);
