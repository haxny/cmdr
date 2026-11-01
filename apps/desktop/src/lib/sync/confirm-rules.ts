/**
 * What the confirm lets a reflex Enter do, and which guards need a second yes. Pure, over the backend's
 * `ConfirmPreview`; the backend re-checks every guard at run time (`gate`), so these rules only decide what the
 * person must press, never what may run.
 *
 * Spec 4.5: Enter accepts only a run that replaces nothing older-over-newer, removes nothing, and needs no
 * acknowledgement; anything else needs the explicit Synchronize button.
 */
import type { Acknowledgement, ConfirmGroupKey, ConfirmPreview, GuardBlock, Side } from '#lib/tauri-commands/sync.js'

/** The side a group writes to (copies land there, replaced and removed files leave it). */
export function groupTarget(key: ConfirmGroupKey): Side {
  switch (key) {
    case 'copyRight':
    case 'copyRightRecreates':
    case 'replaceRight':
    case 'removeRight':
      return 'right'
    case 'copyLeft':
    case 'copyLeftRecreates':
    case 'replaceLeft':
    case 'removeLeft':
      return 'left'
    default: {
      const unreachable: never = key
      return unreachable
    }
  }
}

export function isRemovalGroup(key: ConfirmGroupKey): boolean {
  return key === 'removeRight' || key === 'removeLeft'
}

export function isReplaceGroup(key: ConfirmGroupKey): boolean {
  return key === 'replaceRight' || key === 'replaceLeft'
}

/** Copies of any kind, into whichever side. */
function isCopyGroup(key: ConfirmGroupKey): boolean {
  return !isRemovalGroup(key)
}

/**
 * The second yes each releasable guard asks for: "Remove anyway" when removals on a side pass the limit, "Copy
 * anyway" when copies would fill an empty root. Removals beside an empty root are never releasable (`hardBlocks`).
 */
export function acknowledgementsNeeded(preview: ConfirmPreview): Acknowledgement[] {
  const writes = (side: Side, wanted: (key: ConfirmGroupKey) => boolean) =>
    preview.groups.some((group) => groupTarget(group.key) === side && wanted(group.key))
  const needed = new Set<Acknowledgement>()
  for (const block of preview.blocks) {
    if (block.type === 'maxRemove' && writes(block.side, isRemovalGroup)) needed.add('removeAnyway')
    if (block.type === 'emptyRoot' && writes(block.side, isCopyGroup)) needed.add('copyIntoEmpty')
  }
  return [...needed]
}

/** Guards no button releases: a missing sentinel holds everything, an empty root holds every removal. */
export function hardBlocks(preview: ConfirmPreview): GuardBlock[] {
  return preview.blocks.filter((block) => block.type === 'sentinelMissing' || block.type === 'emptyRoot')
}

/** Whether Enter may accept `accepted` (the ticked groups); otherwise only the Synchronize button does. */
export function enterMayConfirm(preview: ConfirmPreview, accepted: ConfirmGroupKey[]): boolean {
  if (accepted.length === 0) return false
  if (preview.blocks.some((block) => block.type === 'sentinelMissing')) return false
  if (accepted.some(isRemovalGroup)) return false
  if (preview.olderOverwritesNewerTotal > 0 && accepted.some(isReplaceGroup)) return false
  return acknowledgementsNeeded(preview).length === 0
}
