/**
 * What the rail remembers about its tool groups and lines that isn't part of the thread: which
 * group is open, which line is expanded, and what an expanded line fetched.
 *
 * **Keyed by stable ids, outside the message list.** `AskCmdrRail.svelte` keys messages by INDEX,
 * so component-local state would jump to another item on a "load earlier" prepend. A line is its
 * display key (`messageId:callId`), a group its first line's key (`groupKey`). Nothing here is
 * persisted, and none of it lives in `askCmdrState`.
 *
 * **A group opens while its turn runs and settles at the turn's end.** Live, an untouched group is
 * open, and a toggle the person made sticks. The terminal event applies
 * `askCmdr.toolGroupsOnAnswer`: `'collapse'` closes the turn's groups, `'keep'` freezes each as it
 * stands. A reloaded group starts closed under both, unless a line in it still runs.
 *
 * **The raw result is private** (file text, EXIF incl. GPS): fetched only on an expand, held in a
 * `SvelteMap` (which never proxies its values, so it never joins the deep message list), and ❌
 * never logged.
 */

import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { getSetting } from '#lib/settings/index.js'
import { askCmdrToolResult, type AskCmdrToolResult } from '#lib/tauri-commands/index.js'
import type { LineRef, ToolResultDetailView } from '#lib/ipc/bindings.js'
import type { AssistantItem, RailMessage } from './ask-cmdr-messages'
import { displaySegments, groupKey } from './ask-cmdr-rounds'
import { askCmdrState } from './ask-cmdr-state.svelte'

/** What an expanded line has to show. */
export type ToolDetailState =
  | { status: 'loading' }
  | { status: 'ready'; detail: ToolResultDetailView }
  | { status: 'notFound' }
  | { status: 'unavailable' }

/** Groups whose open state is decided: a toggle, or a settled turn. The rest follow the default. */
const groupOpen = new SvelteMap<string, boolean>()
const expandedLines = new SvelteSet<string>()
const details = new SvelteMap<string, ToolDetailState>()

/** Whether the group `key` is open. Undecided, it's open while its turn runs or a line in it
 * does (a reload mid-turn), and closed otherwise. A group with no key yet is open: it can only
 * be the running turn's newest. */
export function isGroupOpen(key: string | null, defaults: { live: boolean; running: boolean }): boolean {
  const decided = key === null ? undefined : groupOpen.get(key)
  return decided ?? (key === null || defaults.live || defaults.running)
}

/** The person opened or closed a group. It sticks until the turn's end applies the setting. */
export function setGroupOpen(key: string, open: boolean): void {
  groupOpen.set(key, open)
}

/** The keys of a bubble's groups, as it renders them. */
function groupKeys(bubble: AssistantItem): string[] {
  return displaySegments(bubble.rounds).flatMap((segment) => {
    const key = segment.kind === 'tools' ? groupKey(segment.lines) : null
    return key === null ? [] : [key]
  })
}

/**
 * A turn ended: its groups close (`'collapse'`), or each keeps what it shows now (`'keep'`: an
 * untouched one was open, so it stays open). Call once its bubble settled.
 */
export function settleTurnGroups(bubble: AssistantItem): void {
  const collapse = getSetting('askCmdr.toolGroupsOnAnswer') === 'collapse'
  for (const key of groupKeys(bubble)) groupOpen.set(key, collapse ? false : (groupOpen.get(key) ?? true))
}

/**
 * The person stopped the turn. Its bubble stops streaming at once, but the turn's end (and the
 * setting) only comes with the backend's terminal, so each untouched group holds the open state
 * it showed until then.
 */
export function holdTurnGroupsOpen(bubble: AssistantItem): void {
  for (const key of groupKeys(bubble)) if (!groupOpen.has(key)) groupOpen.set(key, true)
}

/**
 * A "load earlier" page folded a turn back together (`mergeAtSeam`): the newer half's first group
 * may now continue a group of the older half, under that one's key. Its open state moves along.
 * `head` is the newer list's first bubble before the merge, `merged` the list after it.
 */
export function carryGroupStateAcrossSeam(head: AssistantItem | undefined, merged: RailMessage[]): void {
  if (head?.turnId == null) return
  const firstGroup = displaySegments(head.rounds).find((segment) => segment.kind === 'tools')
  const from = firstGroup?.kind === 'tools' ? groupKey(firstGroup.lines) : null
  const decided = from === null ? undefined : groupOpen.get(from)
  if (from === null || decided === undefined) return
  const bubble = merged.find((item) => item.kind === 'assistant' && item.turnId === head.turnId)
  if (bubble?.kind !== 'assistant') return
  const segment = displaySegments(bubble.rounds).find(
    (candidate) => candidate.kind === 'tools' && candidate.lines.some((line) => line.key === from),
  )
  const to = segment?.kind === 'tools' ? groupKey(segment.lines) : null
  if (to === null || to === from) return
  groupOpen.delete(from)
  groupOpen.set(to, decided)
}

/** Groups whose settled summary was announced, so a re-render (or a remount on a prepend) never
 * says it twice. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state: nothing renders from it.
const announcedGroups = new Set<string>()

/** Claim the one announcement of the group `key`'s settled summary: `true` the first time only. */
export function claimGroupAnnouncement(key: string): boolean {
  if (announcedGroups.has(key)) return false
  announcedGroups.add(key)
  return true
}

/** Whether the line `key` is expanded. */
export function isLineExpanded(key: string): boolean {
  return expandedLines.has(key)
}

/** What the expanded line `key` fetched, `undefined` before its first expand. */
export function toolDetail(key: string): ToolDetailState | undefined {
  return details.get(key)
}

/**
 * Open or close the line `key`, whose call is `ref`. The first open fetches what the call
 * returned; an answer arriving after the person moved to another thread is dropped, so it can't
 * land in a thread that isn't on screen.
 */
export function toggleLineExpanded(key: string, ref: LineRef): void {
  if (expandedLines.delete(key)) return
  expandedLines.add(key)
  const conversationId = askCmdrState.conversationId
  if (details.has(key) || conversationId === null) return
  details.set(key, { status: 'loading' })
  void askCmdrToolResult(conversationId, ref.messageId, ref.callId).then(
    (result) => {
      if (askCmdrState.conversationId !== conversationId) details.delete(key)
      else details.set(key, detailOf(result))
    },
    () => {
      // ❌ Nothing logged: the reply may have carried the private result.
      if (askCmdrState.conversationId !== conversationId) details.delete(key)
      else details.set(key, { status: 'unavailable' })
    },
  )
}

function detailOf(result: AskCmdrToolResult): ToolDetailState {
  if (result.found) return { status: 'ready', detail: result.detail }
  return result.reason === 'notFound' ? { status: 'notFound' } : { status: 'unavailable' }
}

/** Another thread took the screen: what this one's lines fetched goes, so a private result
 * doesn't outlive the thread it belongs to. Which groups were open stays. */
export function forgetToolDetails(): void {
  details.clear()
  expandedLines.clear()
}

/** Test isolation only. */
export function _resetToolGroupsForTests(): void {
  groupOpen.clear()
  expandedLines.clear()
  details.clear()
  announcedGroups.clear()
}
