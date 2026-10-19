// SvelteKit calls `handleError` for every throw it catches: a component that threw while
// rendering, or a `load`. It then swaps the window for `routes/+error.svelte`. Without this hook
// the throw only reached the devtools console, which nobody reads under Tauri.
import type { HandleClientError } from '@sveltejs/kit/hooks'
import { logPageCrash } from '#lib/logging/uncaught-errors.js'

export const handleError: HandleClientError = ({ kind, error, event }) => {
  // SvelteKit's own errors (a route that doesn't exist) aren't crashes, so they stay out of the log.
  if (kind === 'framework') return { detail: error.message }
  return { detail: logPageCrash({ error, routeId: event.route.id }) }
}
