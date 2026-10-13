/** Carrying a typed `AddFavoriteError` from `addFavorite` to the toast that words it. */
import type { AddFavoriteError } from '#lib/ipc/bindings.js'
import { TypedFailure, failureOf } from '#lib/ipc/typed-failure.js'

/** An `Error` that still carries the backend's typed add refusal. */
export class AddFavoriteFailure extends TypedFailure<AddFavoriteError> {
  constructor(failure: AddFavoriteError) {
    super(failure, `add favorite refused: ${failure.type}`)
    this.name = 'AddFavoriteFailure'
  }
}

/** Throws a wire `AddFavoriteError` as an `Error`, keeping the typed value. */
export function throwAddFavoriteError(failure: AddFavoriteError): never {
  throw new AddFavoriteFailure(failure)
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asAddFavoriteError(error: unknown): AddFavoriteError | null {
  return failureOf(AddFavoriteFailure, error)
}
