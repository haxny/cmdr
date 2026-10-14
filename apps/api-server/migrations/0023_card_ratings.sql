-- The triage comments whose `Rating:` line the cron has seen (`report-ratings.ts`), so a rerun is a
-- no-op and a failure alerts once a day rather than every tick. No personal data: GitHub ids, the
-- rating, and timestamps.
CREATE TABLE card_ratings (
    comment_id INTEGER PRIMARY KEY,
    issue_number INTEGER NOT NULL,
    -- The canonical `Rating:` line last applied to the board, NULL until one is. An edit that
    -- changes it re-applies; any other edit to the comment leaves the board alone.
    applied_rating TEXT,
    applied_at TEXT,
    -- Why the last attempt didn't apply, and the UTC day it was last alerted, NULL once it applies.
    last_error TEXT,
    last_error_on TEXT
);
