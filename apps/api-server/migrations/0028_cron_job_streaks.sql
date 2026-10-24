-- One row per every-tick cron job that threw on its latest run and hasn't finished cleanly since. A job alerts only once
-- it has thrown on two ticks in a row (`cron-streaks.ts`), so a one-tick blip (a GitHub 522) stays quiet and redoes its
-- work on the next tick. A clean run deletes the row. No personal data: the job's label from `index.ts` and a tick time.
CREATE TABLE cron_job_streaks (
    job TEXT PRIMARY KEY,
    failed_ticks INTEGER NOT NULL,
    first_failed_at TEXT NOT NULL
);
