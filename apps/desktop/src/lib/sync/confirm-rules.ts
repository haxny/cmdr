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

/** Why a group won't run as things stand: a guard nothing releases, or one waiting for its second yes. */
export type GroupHold = { kind: 'hard'; block: GuardBlock } | { kind: 'ack'; ack: Acknowledgement; block: GuardBlock }

/**
 * The guard holding `key`, mirroring the backend's `gate` (`crates/cmdr-sync/src/guards.rs`): a missing sentinel
 * holds everything, an empty root holds every removal for good, too many removals on a side wait for "Remove
 * anyway", and copies into an empty root wait for "Copy anyway".
 */
export function groupHold(
  preview: ConfirmPreview,
  key: ConfirmGroupKey,
  acknowledged: readonly Acknowledgement[],
): GroupHold | null {
  const side = groupTarget(key)
  const sentinel = preview.blocks.find((block) => block.type === 'sentinelMissing')
  if (sentinel) return { kind: 'hard', block: sentinel }
  if (isRemovalGroup(key)) {
    const empty = preview.blocks.find((block) => block.type === 'emptyRoot')
    if (empty) return { kind: 'hard', block: empty }
    const max = preview.blocks.find((block) => block.type === 'maxRemove' && block.side === side)
    return max && !acknowledged.includes('removeAnyway') ? { kind: 'ack', ack: 'removeAnyway', block: max } : null
  }
  const empty = preview.blocks.find((block) => block.type === 'emptyRoot' && block.side === side)
  return empty && !acknowledged.includes('copyIntoEmpty') ? { kind: 'ack', ack: 'copyIntoEmpty', block: empty } : null
}

/** Groups no button releases: shown held, never ticked. */
export function isHardHeld(preview: ConfirmPreview, key: ConfirmGroupKey): boolean {
  return groupHold(preview, key, [])?.kind === 'hard'
}

/**
 * The second yes each releasable guard asks of `keys` (every group, unless given): "Remove anyway" when removals on a
 * side pass the limit, "Copy anyway" when copies would fill an empty root.
 */
export function acknowledgementsNeeded(
  preview: ConfirmPreview,
  keys: readonly ConfirmGroupKey[] = preview.groups.map((group) => group.key),
): Acknowledgement[] {
  const needed = new Set<Acknowledgement>()
  for (const key of keys) {
    const hold = groupHold(preview, key, [])
    if (hold?.kind === 'ack') needed.add(hold.ack)
  }
  return [...needed]
}

/** Why Synchronize can't run the ticked groups yet, or `null` when it can. */
export type ConfirmBlock =
  | { type: 'nothingTicked' }
  /** Every group is held by a guard no button releases. */
  | { type: 'allHeld' }
  /** Ticked groups wait for a second yes: how many files each acknowledgement holds. */
  | { type: 'waiting'; waits: { ack: Acknowledgement; files: number }[] }

export function confirmBlock(
  preview: ConfirmPreview,
  accepted: readonly ConfirmGroupKey[],
  acknowledged: readonly Acknowledgement[],
): ConfirmBlock | null {
  const runnable = accepted.filter((key) => !isHardHeld(preview, key))
  if (runnable.length === 0) {
    const allHeld = preview.groups.length > 0 && preview.groups.every((group) => isHardHeld(preview, group.key))
    return allHeld ? { type: 'allHeld' } : { type: 'nothingTicked' }
  }
  const files = new Map<Acknowledgement, number>()
  for (const group of preview.groups) {
    if (!runnable.includes(group.key)) continue
    const hold = groupHold(preview, group.key, acknowledged)
    if (hold?.kind === 'ack') files.set(hold.ack, (files.get(hold.ack) ?? 0) + group.files)
  }
  if (files.size === 0) return null
  return { type: 'waiting', waits: [...files].map(([ack, count]) => ({ ack, files: count })) }
}

/** Whether Enter may accept `accepted` (the ticked groups); otherwise only the Synchronize button does. */
export function enterMayConfirm(preview: ConfirmPreview, accepted: ConfirmGroupKey[]): boolean {
  if (accepted.length === 0) return false
  if (accepted.some((key) => isHardHeld(preview, key))) return false
  if (accepted.some(isRemovalGroup)) return false
  if (preview.olderOverwritesNewerTotal > 0 && accepted.some(isReplaceGroup)) return false
  return acknowledgementsNeeded(preview, accepted).length === 0
}
