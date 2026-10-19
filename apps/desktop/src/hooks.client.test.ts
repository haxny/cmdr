/**
 * The client `handleError` hook: a caught throw reaches the log and the error page, and
 * SvelteKit's own "not found" stays out of the log.
 */
import { describe, expect, it, vi } from 'vitest'
import type { NavigationEvent } from '@sveltejs/kit'

const h = vi.hoisted(() => ({ logPageCrash: vi.fn((_: unknown) => 'TypeError: boom') }))
vi.mock('#lib/logging/uncaught-errors.js', () => ({ logPageCrash: h.logPageCrash }))

import { handleError } from './hooks.client'

const event = { route: { id: '/(main)' } } as unknown as NavigationEvent

describe('handleError', () => {
  it('logs a throw and hands its message to the error page', async () => {
    const error = new TypeError('boom')

    expect(await handleError({ kind: 'unknown', error, event })).toEqual({ detail: 'TypeError: boom' })
    expect(h.logPageCrash).toHaveBeenCalledWith({ error, routeId: '/(main)' })
  })

  it("keeps SvelteKit's own errors out of the log, since nothing crashed", async () => {
    h.logPageCrash.mockClear()

    const result = await handleError({ kind: 'framework', error: { status: 404, message: 'Not found' }, event })

    expect(result).toEqual({ detail: 'Not found' })
    expect(h.logPageCrash).not.toHaveBeenCalled()
  })
})
