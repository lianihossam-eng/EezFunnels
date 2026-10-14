CREATE TABLE IF NOT EXISTS command_receipts (
    idempotency_key TEXT PRIMARY KEY NOT NULL CHECK (length(idempotency_key) BETWEEN 1 AND 128),
    command_id TEXT NOT NULL CHECK (length(trim(command_id)) > 0),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS command_receipts_expiry_idx
    ON command_receipts(expires_at);
