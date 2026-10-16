/**
 * The "Delete all Cmdr data" flow behind `UpdatesSection`'s button: summary, confirm, wipe,
 * outcome, quit. The dialog only renders the `view`; the backend does the work
 * (`apps/desktop/src-tauri/src/data_wipe/CLAUDE.md`).
 */

import {
  asDataWipeError,
  deleteAllAppData,
  getDataWipeSummary,
  quitAfterDataWipe,
  type DataWipeSummary,
} from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import type { DataWipeView } from './delete-all-data'

const log = getAppLogger('settings')

export interface DataWipeFlow {
  /** What the dialog shows, or `null` while it's closed. */
  readonly view: DataWipeView | null
  /** The summary is loading, so the button stays dead for that moment. */
  readonly opening: boolean
  open: () => Promise<void>
  confirm: () => Promise<void>
  cancel: () => void
  quit: () => Promise<void>
}

export function createDataWipeFlow(): DataWipeFlow {
  let view = $state<DataWipeView | null>(null)
  let opening = $state(false)

  /** Load the sizes first, so the dialog opens with its numbers instead of growing them. */
  async function open(): Promise<void> {
    if (opening || view !== null) return
    opening = true
    let summary: DataWipeSummary | null = null
    try {
      summary = await getDataWipeSummary()
    } catch (e: unknown) {
      // The dialog says the same things without the numbers.
      log.warn('adding up Cmdr’s data didn’t finish: {error}', { error: String(e) })
    } finally {
      opening = false
    }
    view = { kind: 'confirm', summary, refusal: null }
  }

  async function confirm(): Promise<void> {
    if (view?.kind !== 'confirm') return
    const { summary } = view
    view = { kind: 'deleting', summary }
    try {
      const report = await deleteAllAppData()
      log.info('deleted all Cmdr data, {count} leftover(s)', { count: report.leftovers.length })
      view = { kind: 'done', report }
    } catch (e: unknown) {
      const refusal = asDataWipeError(e)
      if (refusal === null) log.warn('deleting all Cmdr data didn’t answer: {error}', { error: String(e) })
      view = { kind: 'confirm', summary, refusal: refusal ?? { type: 'interrupted' } }
    }
  }

  function cancel(): void {
    if (view?.kind === 'confirm') view = null
  }

  async function quit(): Promise<void> {
    // `false`: a file operation started since, and the main window's quit prompt is asking about
    // it. Nothing to add here; the dialog stays up until the app goes.
    if (!(await quitAfterDataWipe())) log.info('the quit after deleting all data is waiting on a file operation')
  }

  return {
    get view() {
      return view
    },
    get opening() {
      return opening
    },
    open,
    confirm,
    cancel,
    quit,
  }
}
