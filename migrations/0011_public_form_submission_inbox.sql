CREATE INDEX IF NOT EXISTS public_form_submissions_inbox_idx
    ON public_form_submissions(submitted_at DESC, submission_id DESC);
