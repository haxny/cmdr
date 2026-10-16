-- How many times an ops event happened before the digest carried it. A repeating notice (a rejected
-- admin request, a webhook signature that didn't verify) adds to its one unnotified row rather than
-- writing a row per occurrence (`ops-events.ts::countOpsEvent`), so a burst stays one digest line and
-- one small row. Every existing row happened once.
ALTER TABLE ops_events ADD COLUMN occurrences INTEGER NOT NULL DEFAULT 1;
