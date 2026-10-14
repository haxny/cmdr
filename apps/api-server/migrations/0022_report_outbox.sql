-- The report outbox: every crash, error report, amendment, and feedback message gets a card on the
-- private reports board and a line in the three-hour digest, both filed by the cron rather than at
-- intake, so a GitHub or Resend outage delays a report instead of dropping it.
--
-- `issue_number` semantics, shared by every table here:
--   NULL → the cron still owes this row a card
--   0    → never gets one (rows that predate the outbox; debug builds are filtered by query instead)
--   > 0  → the card's issue number in the reports repo

-- One row per uploaded bundle. The bundle itself stays in R2; this row is the bookkeeping plus the
-- two personal fields the cron needs to write the card's expiring comment and the digest card. The
-- retention sweep deletes the row with the bundle's 90 days.
CREATE TABLE error_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    short_id TEXT NOT NULL,          -- ERR-XXXXX, as the client minted it
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    kind TEXT NOT NULL,              -- 'user' | 'auto'
    build_mode TEXT NOT NULL,        -- 'release' | 'debug'
    app_version TEXT NOT NULL,
    os_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    r2_key TEXT NOT NULL,
    user_note TEXT,                  -- personal: deleted with the row after 90 days
    email TEXT,                      -- personal: deleted with the row after 90 days
    issue_number INTEGER,
    notified_at TEXT
);

CREATE INDEX idx_error_reports_short_id ON error_reports(short_id);
CREATE INDEX idx_error_reports_created ON error_reports(created_at);

-- One row per amendment (a note or an address added after the upload). The sidecar in R2 stays the
-- reporter-facing record; this row is what the cron posts to the board and mails. `report_date` is
-- the ORIGINAL upload's day, so the row and its card comment expire with the report it amends.
CREATE TABLE error_report_amendments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    short_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    report_date TEXT NOT NULL,       -- yyyy-mm-dd of the report's upload
    env TEXT NOT NULL,               -- 'prod' | 'dev', from the report index
    r2_key TEXT NOT NULL,
    amendment_count INTEGER NOT NULL,
    note TEXT,                       -- personal: deleted with the row
    email TEXT,                      -- personal: deleted with the row
    board_done_at TEXT,              -- when the amendment reached the board; NULL = still owed
    notified_at TEXT
);

CREATE INDEX idx_error_report_amendments_created ON error_report_amendments(created_at);

-- Crashes and feedback already have their D1 rows; they gain the card pointer. Every existing row is
-- stamped 0 (no card), which is load-bearing: without it the first tick files the whole history.
ALTER TABLE crash_reports ADD COLUMN issue_number INTEGER;
UPDATE crash_reports SET issue_number = 0;

ALTER TABLE feedback ADD COLUMN issue_number INTEGER;
UPDATE feedback SET issue_number = 0;

-- Operational notices that used to go to Discord: CSP violations, a card that missed the backlog
-- board, eviction, the intake budget, signups, a cron job that threw. The digest mails the alerts;
-- the morning brief counts the signups. Nothing personal is ever written here.
CREATE TABLE ops_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    kind TEXT NOT NULL,
    message TEXT NOT NULL,
    notified_at TEXT
);

CREATE INDEX idx_ops_events_created ON ops_events(created_at);
