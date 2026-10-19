/**
 * The client `handleError` hook: a caught throw reaches the log (`logPageCrash`) and comes back
 * with a readable message for the error page and for `DialogManager`'s boundary, in place of
 * SvelteKit's `Internal Error`. SvelteKit's own "not found" stays out of the log.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { NavigationEvent } from '@sveltejs/kit'

const h = vi.hoisted(() => ({ logPageCrash: vi.fn((_: unknown) => 'described') }))
vi.mock('#lib/logging/uncaught-errors.js', () => ({ logPageCrash: h.logPageCrash }))

import { handleError } from './hooks.client'

const event = { route: { id: '/(main)' }, url: new URL('http://localhost/'), params: {} } as unknown as NavigationEvent

beforeEach(() => {
  h.logPageCrash.mockClear()
})

describe('handleError', () => {
  it('logs a throw and hands its message to the error page and to a boundary', async () => {
    const error = new TypeError('each_key_duplicate', { cause: new Error('key "a" twice') })
    h.logPageCrash.mockReturnValueOnce('TypeError: each_key_duplicate; caused by Error: key "a" twice')

    const result = await handleError({ kind: 'unknown', error, event })

    expect(result).toEqual({
      message: 'TypeError: each_key_duplicate',
      detail: 'TypeError: each_key_duplicate; caused by Error: key "a" twice',
    })
    expect(h.logPageCrash).toHaveBeenCalledWith({ error, routeId: '/(main)' })
  })

  it('describes a thrown non-Error too', async () => {
    h.logPageCrash.mockReturnValueOnce('{"type":"boom"}')

    const result = await handleError({ kind: 'unknown', error: { type: 'boom' }, event })

    expect(result).toEqual({ message: '{"type":"boom"}', detail: '{"type":"boom"}' })
  })

  it('falls back to the path when no route matched', async () => {
    const noRoute = { route: { id: null }, url: new URL('http://localhost/settings'), params: {} } as unknown as NavigationEvent

    await handleError({ kind: 'unknown', error: new Error('boom'), event: noRoute })

    expect(h.logPageCrash).toHaveBeenCalledWith({ error: expect.any(Error), routeId: '/settings' })
  })

  it("keeps an app error's own message", async () => {
    const result = await handleError({ kind: 'app', error: { status: 403, message: 'Nope' }, event })

    expect(result).toEqual({ detail: 'described' })
    expect(h.logPageCrash).toHaveBeenCalledTimes(1)
  })

  it("keeps SvelteKit's own errors out of the log, since nothing crashed", async () => {
    const result = await handleError({ kind: 'framework', error: { status: 404, message: 'Not found' }, event })

    expect(result).toEqual({ detail: 'Not found' })
    expect(h.logPageCrash).not.toHaveBeenCalled()
  })
})
