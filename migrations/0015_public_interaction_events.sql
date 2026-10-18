CREATE TABLE IF NOT EXISTS public_interaction_events (
    event_id TEXT PRIMARY KEY NOT NULL CHECK (length(event_id) = 36),
    funnel_id TEXT NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    revision_id TEXT NOT NULL REFERENCES revisions(id) ON DELETE CASCADE,
    page_id TEXT NOT NULL CHECK (length(page_id) BETWEEN 1 AND 64),
    event_kind TEXT NOT NULL CHECK (event_kind IN ('page_viewed', 'form_viewed', 'form_started')),
    form_id TEXT CHECK (form_id IS NULL OR length(form_id) BETWEEN 1 AND 64),
    campaign_id TEXT CHECK (campaign_id IS NULL OR length(campaign_id) BETWEEN 1 AND 64),
    replay_fingerprint TEXT NOT NULL CHECK (length(replay_fingerprint) = 64),
    received_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL CHECK (expires_at > received_at),
    CHECK (
        (event_kind = 'page_viewed' AND form_id IS NULL)
        OR (event_kind IN ('form_viewed', 'form_started') AND form_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS public_interaction_events_expiry_idx
    ON public_interaction_events(expires_at, event_id);

CREATE INDEX IF NOT EXISTS public_interaction_events_rate_idx
    ON public_interaction_events(funnel_id, received_at);

CREATE INDEX IF NOT EXISTS public_interaction_events_report_idx
    ON public_interaction_events(funnel_id, received_at, event_kind, campaign_id);

CREATE TRIGGER IF NOT EXISTS public_interaction_event_must_match_current_revision
BEFORE INSERT ON public_interaction_events
WHEN NOT EXISTS (
    SELECT 1
    FROM funnels
    JOIN revisions ON revisions.id = funnels.published_revision_id
        AND revisions.funnel_id = funnels.id
    WHERE funnels.id = NEW.funnel_id
        AND funnels.published_revision_id = NEW.revision_id
        AND revisions.schema_version IN (1, 2)
)
BEGIN
    SELECT RAISE(ABORT, 'interaction event must match the current published revision');
END;
