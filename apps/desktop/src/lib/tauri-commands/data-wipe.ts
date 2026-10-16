// "Delete all Cmdr data" (Settings › Updates & privacy). The backend owns what gets deleted
// and how: `apps/desktop/src-tauri/src/data_wipe/CLAUDE.md`.

import { commands, type DataWipeError, type DataWipeSummary, type WipeReport } from '#lib/ipc/bindings.js'
import { TypedFailure } from '#lib/ipc/typed-failure.js'

export type { DataWipeError, DataWipeSummary, WipeLeftover, WipeReport } from '#lib/ipc/bindings.js'

/** A wipe (or its summary) that didn't happen, still carrying the backend's typed reason. */
export class DataWipeFailure extends TypedFailure<DataWipeError> {
  constructor(failure: DataWipeError) {
    super(failure, `data wipe refused: ${failure.type}`)
    this.name = 'DataWipeFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asDataWipeError(error: unknown): DataWipeError | null {
  return error instanceof DataWipeFailure ? error.failure : null
}

/** How much would go, and whether a license is activated on this Mac. */
export async function getDataWipeSummary(): Promise<DataWipeSummary> {
  const res = await commands.dataWipeSummary()
  if (res.status === 'error') throw new DataWipeFailure(res.error)
  return res.data
}

/** Delete everything Cmdr keeps on this Mac, and say what couldn't go. Nothing to undo. */
export async function deleteAllAppData(): Promise<WipeReport> {
  const res = await commands.deleteAllAppData()
  if (res.status === 'error') throw new DataWipeFailure(res.error)
  return res.data
}

/**
 * Quit after a wipe, through the quit gate. `false` when a file operation started since and the
 * gate is asking about it instead.
 */
export async function quitAfterDataWipe(): Promise<boolean> {
  return await commands.quitAfterDataWipe()
}
