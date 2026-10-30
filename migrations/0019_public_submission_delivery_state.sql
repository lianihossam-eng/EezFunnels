CREATE TABLE IF NOT EXISTS public_submission_delivery_state (
    task_id TEXT PRIMARY KEY NOT NULL
        REFERENCES public_submission_delivery_outbox(task_id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'leased', 'accepted', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count BETWEEN 0 AND 5),
    next_attempt_at INTEGER NOT NULL,
    lease_token TEXT,
    lease_expires_at INTEGER,
    last_attempt_at INTEGER,
    last_http_status INTEGER
        CHECK (last_http_status IS NULL OR last_http_status BETWEEN 100 AND 599),
    last_outcome_code TEXT
        CHECK (last_outcome_code IS NULL OR length(last_outcome_code) BETWEEN 1 AND 40),
    completed_at INTEGER,
    updated_at INTEGER NOT NULL,
    CHECK ((lease_token IS NULL) = (lease_expires_at IS NULL)),
    CHECK ((status = 'leased') = (lease_token IS NOT NULL)),
    CHECK ((status IN ('accepted', 'failed')) = (completed_at IS NOT NULL)),
    CHECK (attempt_count = 0 OR last_attempt_at IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS public_submission_delivery_state_due_idx
    ON public_submission_delivery_state(status, next_attempt_at, task_id);

CREATE INDEX IF NOT EXISTS public_submission_delivery_state_lease_idx
    ON public_submission_delivery_state(status, lease_expires_at, task_id);

INSERT OR IGNORE INTO public_submission_delivery_state (
    task_id, status, attempt_count, next_attempt_at, updated_at
)
SELECT task_id, 'pending', 0, created_at, created_at
FROM public_submission_delivery_outbox;

CREATE TRIGGER IF NOT EXISTS public_submission_delivery_state_create
AFTER INSERT ON public_submission_delivery_outbox
BEGIN
    INSERT INTO public_submission_delivery_state (
        task_id, status, attempt_count, next_attempt_at, updated_at
    ) VALUES (
        NEW.task_id, 'pending', 0, NEW.created_at, NEW.created_at
    );
END;
