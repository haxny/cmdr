/**
 * The words of the "Delete all Cmdr data" dialog (`DeleteAllDataDialog.svelte`), worked out
 * from the backend's typed summary, refusal, and report. Pure, so the copy each case gets is
 * tested without mounting anything. What gets deleted, and how: the Rust `data_wipe` module.
 */

import type { DataWipeError, DataWipeSummary, WipeLeftover, WipeReport } from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { formatByteSize } from '#lib/units/index.js'

/** What the dialog shows. `summary` is `null` when the sizes couldn't be added up in time. */
export type DataWipeView =
  | { kind: 'confirm'; summary: DataWipeSummary | null; refusal: DataWipeError | null }
  | { kind: 'deleting'; summary: DataWipeSummary | null }
  | { kind: 'done'; report: WipeReport }

export function introText(summary: DataWipeSummary | null): string {
  return summary && summary.totalBytes > 0
    ? tString('settings.dataWipe.introWithSize', { sizeText: formatByteSize(summary.totalBytes) })
    : tString('settings.dataWipe.intro')
}

/**
 * The "What goes" list, in the person's terms. The license line shows only when one is
 * activated (or when that's unknown, in its "if you activated one" form), and the models line
 * carries their size when there are any.
 */
export function goesItems(summary: DataWipeSummary | null): string[] {
  const items = [
    tString('settings.dataWipe.goes.settings'),
    tString('settings.dataWipe.goes.servers'),
    tString('settings.dataWipe.goes.apiKeys'),
  ]
  if (summary === null) items.push(tString('settings.dataWipe.goes.licenseIfAny'))
  else if (summary.hasLicense) items.push(tString('settings.dataWipe.goes.license'))
  items.push(
    tString('settings.dataWipe.goes.askCmdr'),
    tString('settings.dataWipe.goes.histories'),
    tString('settings.dataWipe.goes.operationLog'),
    tString('settings.dataWipe.goes.indexes'),
  )
  if (summary === null) items.push(tString('settings.dataWipe.goes.models'))
  else if (summary.modelBytes > 0) {
    items.push(tString('settings.dataWipe.goes.modelsWithSize', { sizeText: formatByteSize(summary.modelBytes) }))
  }
  items.push(tString('settings.dataWipe.goes.logs'))
  return items
}

/** Why nothing happened (or why it stopped), as one or two plain sentences. */
export function refusalText(refusal: DataWipeError): string {
  switch (refusal.type) {
    case 'fileOperationsRunning':
      return tString('settings.dataWipe.refused.operationsRunning', {
        count: refusal.count,
        countText: formatInteger(refusal.count),
      })
    case 'alreadyRunning':
      return tString('settings.dataWipe.refused.alreadyRunning')
    // `timedOut` only ever answers the summary, which the dialog survives without; listed so
    // the switch stays exhaustive.
    case 'interrupted':
    case 'timedOut':
      return tString('settings.dataWipe.refused.interrupted')
  }
}

/** One line of the "remove these yourself" list. A path stays verbatim: it's what Finder's Go to Folder takes. */
export function leftoverText(leftover: WipeLeftover): string {
  switch (leftover.type) {
    case 'path':
      return leftover.path
    case 'secretItems':
      return tString('settings.dataWipe.leftover.secretItems', {
        count: leftover.count,
        countText: formatInteger(leftover.count),
        itemName: leftover.itemName,
      })
    case 'secretsUnreadable':
      return tString('settings.dataWipe.leftover.secretsUnreadable', { itemName: leftover.itemName })
  }
}
