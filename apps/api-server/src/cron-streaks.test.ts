import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { clearCronStreak, recordCronFailure } from './cron-streaks'
import { createSqliteD1 } from './sqlite-d1-test-helper'

const job = 'Card ratings'
const firstTick = '2026-10-24T18:00:00.000Z'
const secondTick = '2026-10-24T21:00:00.000Z'

beforeEach(() => {
  vi.spyOn(console, 'error').mockImplementation(() => undefined)
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('recordCronFailure', () => {
  it('holds the first failure back and alerts on the second in a row', async () => {
    const { d1 } = createSqliteD1()

    await expect(recordCronFailure(d1, job, firstTick, 2)).resolves.toEqual({
      alert: false,
      failedTicks: 1,
      firstFailedAt: firstTick,
    })
    await expect(recordCronFailure(d1, job, secondTick, 2)).resolves.toEqual({
      alert: true,
      failedTicks: 2,
      firstFailedAt: firstTick,
    })
  })

  it('keeps alerting while the job keeps failing', async () => {
    const { d1 } = createSqliteD1()
    await recordCronFailure(d1, job, firstTick, 2)
    await recordCronFailure(d1, job, secondTick, 2)

    const third = await recordCronFailure(d1, job, '2026-10-25T00:00:00.000Z', 2)

    expect(third).toEqual({ alert: true, failedTicks: 3, firstFailedAt: firstTick })
  })

  it('starts over after a clean run, so failures on alternate ticks never alert', async () => {
    const { d1 } = createSqliteD1()
    await recordCronFailure(d1, job, firstTick, 2)

    await clearCronStreak(d1, job)

    await expect(recordCronFailure(d1, job, secondTick, 2)).resolves.toMatchObject({ alert: false, failedTicks: 1 })
  })

  it("counts each job's streak on its own", async () => {
    const { d1 } = createSqliteD1()
    await recordCronFailure(d1, job, firstTick, 2)

    await expect(recordCronFailure(d1, 'Crash cards', secondTick, 2)).resolves.toMatchObject({ alert: false })
  })

  it('alerts on the first failure when the job has no grace, without touching the database', async () => {
    const prepare = vi.fn(() => {
      throw new Error('should not be called')
    })

    const strike = await recordCronFailure({ prepare } as unknown as D1Database, job, firstTick, 1)

    expect(strike).toEqual({ alert: true, failedTicks: 1, firstFailedAt: firstTick })
    expect(prepare).not.toHaveBeenCalled()
  })

  it("alerts at once when the streak can't be read, so a broken D1 can't hide a failure", async () => {
    const broken = {
      prepare: () => {
        throw new Error('D1_ERROR: no such table: cron_job_streaks')
      },
    } as unknown as D1Database

    await expect(recordCronFailure(broken, job, firstTick, 2)).resolves.toMatchObject({ alert: true })
    await expect(recordCronFailure(undefined, job, firstTick, 2)).resolves.toMatchObject({ alert: true })
  })
})

describe('clearCronStreak', () => {
  it('never throws, so a broken D1 costs a clean job nothing', async () => {
    const broken = {
      prepare: () => {
        throw new Error('D1_ERROR')
      },
    } as unknown as D1Database

    await expect(clearCronStreak(broken, job)).resolves.toBeUndefined()
  })
})
