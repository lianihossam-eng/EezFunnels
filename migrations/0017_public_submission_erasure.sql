CREATE TABLE IF NOT EXISTS public_submission_erasure_intents (
    submission_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(submission_id) = 36),
    replay_digest TEXT NOT NULL
        CHECK (length(replay_digest) = 64)
        CHECK (replay_digest NOT GLOB '*[^0-9a-f]*'),
    source_expires_at INTEGER NOT NULL,
    deleted_at INTEGER NOT NULL,
    retain_until INTEGER NOT NULL
        CHECK (retain_until >= source_expires_at)
        CHECK (retain_until >= deleted_at),
    last_attempt_at INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS public_submission_erasure_pending_idx
    ON public_submission_erasure_intents(last_attempt_at, deleted_at, submission_id);

CREATE TABLE IF NOT EXISTS public_submission_erasure_markers (
    replay_digest TEXT PRIMARY KEY NOT NULL
        CHECK (length(replay_digest) = 64)
        CHECK (replay_digest NOT GLOB '*[^0-9a-f]*'),
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS public_submission_erasure_marker_expiry_idx
    ON public_submission_erasure_markers(expires_at, replay_digest);
