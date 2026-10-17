CREATE INDEX IF NOT EXISTS public_form_submissions_funnel_inbox_idx
    ON public_form_submissions(funnel_id, submitted_at DESC, submission_id DESC);

CREATE INDEX IF NOT EXISTS public_form_submissions_funnel_status_inbox_idx
    ON public_form_submissions(funnel_id, triage_status, submitted_at DESC, submission_id DESC);
