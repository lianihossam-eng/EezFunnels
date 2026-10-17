ALTER TABLE public_form_submissions
    ADD COLUMN submission_channel TEXT NOT NULL DEFAULT 'unattributed'
    CHECK (submission_channel IN (
        'unattributed', 'search', 'social', 'email', 'paid', 'partner', 'other'
    ));
