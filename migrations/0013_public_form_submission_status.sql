ALTER TABLE public_form_submissions
    ADD COLUMN triage_status TEXT NOT NULL DEFAULT 'new'
    CHECK (triage_status IN ('new', 'contacted', 'qualified', 'closed'));

CREATE INDEX IF NOT EXISTS public_form_submissions_status_inbox_idx
    ON public_form_submissions(triage_status, submitted_at DESC, submission_id DESC);
