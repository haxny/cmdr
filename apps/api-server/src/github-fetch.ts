/**
 * `fetch` for the GitHub API that rides out a brief outage on reads.
 *
 * GitHub (or the path to it) now and then answers a request with a 5xx, a 522 among them, and is
 * fine a second later (seen on "Card ratings", 2026-10-24). A read that fails like that would throw
 * its whole cron job for the tick, so a read gets two more tries a moment apart. A 4xx is GitHub
 * meaning it, and is never retried.
 *
 * Only idempotent requests retry: GET, HEAD, PUT, DELETE, and a POST the caller vouches for (a
 * GraphQL query). A POST that creates something may have landed before its 5xx, and a retry would
 * file a second issue or comment.
 */
import { pause } from './retry-pause'

/**
 * The waits before the second and third tries. A cron tick or background work can afford four
 * seconds; a person waiting on a response can't, so keep this off those paths.
 */
const RETRY_DELAYS_MS = [1_000, 3_000]

const IDEMPOTENT_METHODS = new Set(['GET', 'HEAD', 'PUT', 'DELETE'])

export async function githubFetch(
  url: string,
  init: RequestInit,
  options: { idempotent?: boolean } = {},
): Promise<Response> {
  const idempotent = options.idempotent ?? IDEMPOTENT_METHODS.has((init.method ?? 'GET').toUpperCase())
  if (!idempotent) return fetch(url, init)

  for (const delay of RETRY_DELAYS_MS) {
    try {
      const response = await fetch(url, init)
      if (response.status < 500) return response
      console.error(`GitHub answered ${String(response.status)} on ${redact(url)}; retrying in ${String(delay)} ms`)
    } catch (e) {
      console.error(`GitHub request to ${redact(url)} threw; retrying in ${String(delay)} ms`, e)
    }
    await pause(delay)
  }
  return fetch(url, init)
}

/** The URL without its query string, which can carry a search the logs don't need. */
function redact(url: string): string {
  const query = url.indexOf('?')
  return query === -1 ? url : url.slice(0, query)
}
