-- One row per day: the telemetry D1's own size, as `meta.size_after` read it at the 00:00 UTC tick. The DB size check
-- compares today with the latest earlier sample so it alerts once per step crossed instead of every day, and reads the
-- growth rate off a sample about a month old. No personal data; `handleDbSizeCheck` prunes rows past 400 days.
CREATE TABLE db_size_samples (
    day TEXT PRIMARY KEY,
    size_bytes INTEGER NOT NULL
);
