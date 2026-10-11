/**
 * Listing accessors refuse a gone listing or a changed row revision.
 *
 * Every listing-read wrapper in `file-listing.ts` throws through
 * `throwListingLookupError`, which reports only `gone` to listeners, so the pane showing
 * that listing can re-list instead of serving stale rows to every later command.
 * The listener that acts on it is `file-explorer/pane/listing-liveness.ts`.
 */
import type { ListingLookupError } from '$lib/ipc/bindings'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'

/** A typed listing refusal, preserved across the throw for revision-aware callers. */
export class ListingLookupFailure extends TypedFailure<ListingLookupError> {
  constructor(failure: ListingLookupError) {
    super(failure, `Listing ${failure.listingId}: ${failure.type}`)
  }
}

/**
 * The listing refusal behind a caught value, or `null` for any other error. Either
 * variant (`gone`, `changed`) means the caller's row indices no longer name rows.
 */
export function asListingLookupError(error: unknown): ListingLookupError | null {
  return failureOf(ListingLookupFailure, error)
}

const listeners = new Set<(listingId: string) => void>()

/** Hears the id of every listing a read found gone. Returns the unsubscribe. */
export function onListingGone(listener: (listingId: string) => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** Throws a listing accessor's typed refusal, first telling every `onListingGone` listener. */
export function throwListingLookupError(error: ListingLookupError): never {
  if (error.type === 'gone') for (const listener of listeners) listener(error.listingId)
  throw new ListingLookupFailure(error)
}
