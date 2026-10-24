/**
 * How many ticks in a row each cron job has thrown, so a one-tick blip stays quiet.
 *
 * Every job redoes its own work on the next tick, and none of it is urgent within three hours, so
 * a job that throws once (GitHub answering a page with a 522) needn't wake anyone. A job with
 * grace alerts once it has thrown on `alertAfter` ticks in a row, and on every failing tick after
 * that. A clean run starts the count over.
 *
 * Fails toward alerting: if the streak can't be read or written (D1 is down), the failure alerts
 * at once. A broken D1 must never be what hides a broken cron. Neither function throws.
 */

/** Ticks in a row an every-tick job may throw before it alerts: one blip is free. */
export const EVERY_TICK_ALERT_AFTER = 2

export interface CronFailureStreak {
  /** Whether this failure should reach the digest and healthchecks.io now. */
  alert: boolean
  /** Ticks in a row this job has thrown, this one included. */
  failedTicks: number
  /** The `scheduledTime` (ISO 8601) of the first tick in the streak. */
  firstFailedAt: string
}

/**
 * Count one more failed tick for `job` and say whether it alerts. `alertAfter` 1 means no grace:
 * the failure alerts and the table isn't touched (the daily jobs, whose next run is a day away).
 */
export async function recordCronFailure(
  db: D1Database | undefined,
  job: string,
  when: string,
  alertAfter: number,
): Promise<CronFailureStreak> {
  const unknownStreak = { alert: true, failedTicks: 1, firstFailedAt: when }
  if (alertAfter <= 1 || !db) return unknownStreak
  try {
    const row = await db
      .prepare(
        `INSERT INTO cron_job_streaks (job, failed_ticks, first_failed_at) VALUES (?, 1, ?)
         ON CONFLICT (job) DO UPDATE SET failed_ticks = failed_ticks + 1
         RETURNING failed_ticks, first_failed_at`,
      )
      .bind(job, when)
      .first<{ failed_ticks: number; first_failed_at: string }>()
    if (!row) return unknownStreak
    return { alert: row.failed_ticks >= alertAfter, failedTicks: row.failed_ticks, firstFailedAt: row.first_failed_at }
  } catch (e) {
    console.error(`Cron streak for "${job}" could not be counted, so this failure alerts now`, e)
    return unknownStreak
  }
}

/** A clean run: forget the job's streak. */
export async function clearCronStreak(db: D1Database | undefined, job: string): Promise<void> {
  if (!db) return
  try {
    await db.prepare(`DELETE FROM cron_job_streaks WHERE job = ?`).bind(job).run()
  } catch (e) {
    console.error(`Cron streak for "${job}" could not be cleared`, e)
  }
}
