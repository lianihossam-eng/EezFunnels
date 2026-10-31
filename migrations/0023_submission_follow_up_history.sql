CREATE TABLE public_submission_follow_up_history (
    history_id INTEGER PRIMARY KEY CHECK (history_id = 1),
    history_started_at INTEGER NOT NULL CHECK (history_started_at > 0)
);

INSERT INTO public_submission_follow_up_history (history_id, history_started_at)
VALUES (1, unixepoch());

CREATE TRIGGER public_submission_follow_up_history_immutable_insert
BEFORE INSERT ON public_submission_follow_up_history
WHEN EXISTS (
    SELECT 1 FROM public_submission_follow_up_history WHERE history_id = 1
)
BEGIN
    SELECT RAISE(ABORT, 'submission follow-up history boundary is immutable');
END;

CREATE TRIGGER public_submission_follow_up_history_immutable_update
BEFORE UPDATE ON public_submission_follow_up_history
BEGIN
    SELECT RAISE(ABORT, 'submission follow-up history boundary is immutable');
END;

CREATE TRIGGER public_submission_follow_up_history_immutable_delete
BEFORE DELETE ON public_submission_follow_up_history
BEGIN
    SELECT RAISE(ABORT, 'submission follow-up history boundary is immutable');
END;
