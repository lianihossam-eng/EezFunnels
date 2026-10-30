ALTER TABLE public_form_submissions
    ADD COLUMN follow_up_version INTEGER NOT NULL DEFAULT 0
    CHECK (follow_up_version BETWEEN 0 AND 2147483646);

ALTER TABLE public_form_submissions
    ADD COLUMN next_action TEXT
    CHECK (next_action IS NULL OR length(CAST(next_action AS BLOB)) BETWEEN 1 AND 512);

ALTER TABLE public_form_submissions
    ADD COLUMN due_on TEXT
    CHECK (due_on IS NULL OR (length(due_on) = 10 AND due_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'));

ALTER TABLE public_form_submissions
    ADD COLUMN close_reason TEXT
    CHECK (
        (close_reason IS NULL OR length(CAST(close_reason AS BLOB)) BETWEEN 1 AND 512)
        AND (close_reason IS NULL OR triage_status = 'closed')
    );

CREATE TABLE public_submission_follow_up_events (
    submission_id TEXT NOT NULL REFERENCES public_form_submissions(submission_id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 2147483646),
    status TEXT NOT NULL CHECK (status IN ('new', 'contacted', 'qualified', 'closed')),
    next_action TEXT CHECK (next_action IS NULL OR length(CAST(next_action AS BLOB)) BETWEEN 1 AND 512),
    due_on TEXT CHECK (due_on IS NULL OR (length(due_on) = 10 AND due_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')),
    close_reason TEXT CHECK (close_reason IS NULL OR length(CAST(close_reason AS BLOB)) BETWEEN 1 AND 512),
    recorded_at INTEGER NOT NULL,
    provenance TEXT NOT NULL CHECK (provenance IN ('owner_follow_up', 'legacy_status')),
    supersedes_version INTEGER,
    corrects_version INTEGER,
    PRIMARY KEY (submission_id, version),
    FOREIGN KEY (submission_id, supersedes_version)
        REFERENCES public_submission_follow_up_events(submission_id, version),
    FOREIGN KEY (submission_id, corrects_version)
        REFERENCES public_submission_follow_up_events(submission_id, version),
    CHECK (close_reason IS NULL OR status = 'closed'),
    CHECK (
        (version = 1 AND supersedes_version IS NULL)
        OR (version > 1 AND supersedes_version = version - 1)
    ),
    CHECK (corrects_version IS NULL OR corrects_version < version)
);

CREATE UNIQUE INDEX public_submission_follow_up_events_one_correction_idx
    ON public_submission_follow_up_events(submission_id, corrects_version)
    WHERE corrects_version IS NOT NULL;

CREATE TRIGGER public_submission_follow_up_events_corrects_ordinary_event
BEFORE INSERT ON public_submission_follow_up_events
WHEN NEW.corrects_version IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM public_submission_follow_up_events AS target
    WHERE target.submission_id = NEW.submission_id
        AND target.version = NEW.corrects_version
        AND target.corrects_version IS NULL
)
BEGIN
    SELECT RAISE(ABORT, 'submission follow-up correction target must be an ordinary event');
END;

CREATE TRIGGER public_submission_follow_up_events_immutable
BEFORE UPDATE ON public_submission_follow_up_events
BEGIN
    SELECT RAISE(ABORT, 'submission follow-up events are immutable');
END;

CREATE TRIGGER public_submission_follow_up_events_append_only
BEFORE DELETE ON public_submission_follow_up_events
WHEN EXISTS (
    SELECT 1 FROM public_form_submissions WHERE submission_id = OLD.submission_id
)
BEGIN
    SELECT RAISE(ABORT, 'submission follow-up events are append-only');
END;

CREATE TRIGGER public_submission_follow_up_events_match_current_version
BEFORE INSERT ON public_submission_follow_up_events
WHEN NOT EXISTS (
    SELECT 1
    FROM public_form_submissions AS source
    WHERE source.submission_id = NEW.submission_id
        AND NEW.version = source.follow_up_version + 1
        AND source.expires_at > unixepoch()
        AND NOT EXISTS (
            SELECT 1
            FROM public_submission_erasure_intents AS intent
            WHERE intent.submission_id = source.submission_id
        )
)
BEGIN
    SELECT RAISE(ABORT, 'submission follow-up event must extend a retained current state');
END;

CREATE INDEX public_submission_follow_up_events_timeline_idx
    ON public_submission_follow_up_events(submission_id, version);

CREATE TABLE public_submission_follow_up_receipts (
    submission_id TEXT NOT NULL REFERENCES public_form_submissions(submission_id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL CHECK (length(idempotency_key) BETWEEN 1 AND 128),
    request_digest TEXT NOT NULL
        CHECK (length(request_digest) = 64)
        CHECK (request_digest NOT GLOB '*[^0-9a-f]*'),
    result_version INTEGER NOT NULL CHECK (result_version >= 0),
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (idempotency_key)
);

CREATE INDEX public_submission_follow_up_receipts_expiry_idx
    ON public_submission_follow_up_receipts(expires_at, submission_id);

CREATE TRIGGER public_submission_follow_up_receipts_respect_retention
BEFORE INSERT ON public_submission_follow_up_receipts
WHEN NEW.expires_at > (
    SELECT expires_at FROM public_form_submissions WHERE submission_id = NEW.submission_id
)
BEGIN
    SELECT RAISE(ABORT, 'follow-up receipts cannot outlive the submission');
END;
