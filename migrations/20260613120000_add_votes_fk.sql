-- Add foreign key constraints to enforce referential integrity.
-- SQLite requires PRAGMA foreign_keys = ON to enforce these (already set in
-- SqliteConnectOptions). Existing data is assumed to be consistent.

CREATE TABLE IF NOT EXISTS votes_new (
    client_uuid TEXT NOT NULL,
    model_id    TEXT NOT NULL,
    ip_address  TEXT NOT NULL,
    voted_at    DATETIME NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (client_uuid, model_id),
    FOREIGN KEY (model_id) REFERENCES models(model_id) ON DELETE CASCADE
);

INSERT OR REPLACE INTO votes_new SELECT * FROM votes;
DROP TABLE votes;
ALTER TABLE votes_new RENAME TO votes;

CREATE INDEX IF NOT EXISTS idx_votes_ip_time
    ON votes(ip_address, voted_at);
