/**
 * The Album pane over the Tracks pane's scope (`TrackSelection.scoped`): the selected tracks, else, with none selected,
 * the ones the Tracks filter shows. The backend sums up only those tracks in the view's `shared` rows (`setViewScope`,
 * `scopeCount`), and an Album edit sets only those tracks. Every track while neither narrows it. The pane never shows
 * one scope while an edit goes to another: every Album edit first `flush`es a scope change still waiting out its
 * delay, so the view shows the new scope before the edit is sent.
 */

import type { TagEdit } from '#lib/tauri-commands/index.js'

/** How long the selection rests before the Album pane follows it: a held ⇧↓ sends one call, not one per row. */
export const VIEW_SCOPE_DELAY_MS = 120

/**
 * An Album pane edit (a typed value, a picked choice, Clear, Delete, paste) for `scope`: each `setAll` becomes a
 * `setOne` per scoped track whose tags were read (`readable`); `null` (nothing selected) keeps the edits as they are.
 */
export function scopedAlbumEdits(edits: TagEdit[], scope: string[] | null, readable: ReadonlySet<string>): TagEdit[] {
  if (scope === null) return edits
  const paths = scope.filter((path) => readable.has(path))
  return edits.flatMap((edit): TagEdit[] =>
    edit.kind === 'setAll'
      ? paths.map((path) => ({ kind: 'setOne', path, field: edit.field, values: edit.values }))
      : [edit],
  )
}

export interface ViewScopeDeps {
  /** The scoped tracks' paths: the selected ones, else the filter's shown ones, else none (every track). */
  selected: () => ReadonlySet<string>
  /** How many tracks the view shown covers (`view.scopeCount`, `null` for all); `undefined` while there is no view. */
  shownCount: () => number | null | undefined
  /** Resolves once the calls out have answered, so a scope is never asked while an edit is on its way. */
  settled: () => Promise<unknown>
  /** Sets the backend's view scope (`null` for every track); resolves whether the backend took it. */
  setViewScope: (paths: string[] | null) => Promise<boolean>
}

export interface ViewScopeSync {
  /**
   * Sends a selection change still waiting out its delay (or still out) now, and resolves once the view shows it;
   * `null` when the view already shows the selection, so an edit goes on at once (a write right after it still sees
   * it out).
   */
  flush(): Promise<void> | null
}

const NOTHING = JSON.stringify([])

function pathsOf(key: string): string[] {
  return JSON.parse(key) as string[]
}

/** How many tracks the view covers for selection `key`: `null` (every track) when none is selected. */
function countOf(key: string): number | null {
  const count = pathsOf(key).length
  return count > 0 ? count : null
}

/**
 * Keeps the backend's view scope on the selected tracks: one `setViewScope` per selection change, after
 * `VIEW_SCOPE_DELAY_MS` of quiet and once the calls out have answered. Keyed by the paths alone, so a sort (a new
 * order) sends nothing; an emptied or vanished selection widens it to every track (`null`). A newest view whose
 * scope doesn't match the selection (an answer lost to a failure) sends it again, once. Call it while a component
 * initializes (it owns two `$effect`s).
 */
export function followViewScope(deps: ViewScopeDeps): ViewScopeSync {
  const key = $derived(JSON.stringify([...deps.selected()].sort()))
  /** The selection the backend last took; nothing selected at first, which a fresh view already shows. */
  let sent = NOTHING
  /** The selection sent again for a view that didn't match it: never twice for one selection. */
  let resentFor: string | null = null
  let timer: ReturnType<typeof setTimeout> | undefined
  let inFlight: { key: string; done: Promise<void> } | null = null

  /** Sends selection `next` once the calls out have answered; `again` sends it even though the backend took it. */
  async function send(next: string, again: boolean): Promise<void> {
    await deps.settled()
    // A newer selection came meanwhile: it sends itself.
    if (next !== key || (next === sent && !again)) return
    const paths = pathsOf(next)
    if (await deps.setViewScope(paths.length > 0 ? paths : null)) sent = next
  }

  function start(next: string, again = false): Promise<void> {
    if (inFlight?.key === next) return inFlight.done
    const done = send(next, again).finally(() => {
      if (inFlight?.done === done) inFlight = null
      // A selection that changed back while this one was out is sent too, unless its own timer still runs.
      if (sent === next && key !== sent && timer === undefined) void start(key)
    })
    inFlight = { key: next, done }
    return done
  }

  $effect(() => {
    const next = key
    if (next === sent) return
    timer = setTimeout(() => {
      timer = undefined
      void start(next)
    }, VIEW_SCOPE_DELAY_MS)
    return () => {
      clearTimeout(timer)
      timer = undefined
    }
  })

  $effect(() => {
    const shown = deps.shownCount()
    const current = key
    if (shown === undefined || current !== sent || timer !== undefined || inFlight !== null) return
    if (shown === countOf(current)) {
      resentFor = null
      return
    }
    if (resentFor === current) return
    resentFor = current
    void start(current, true)
  })

  return {
    flush() {
      if (timer === undefined && inFlight === null && key === sent) return null
      clearTimeout(timer)
      timer = undefined
      return (async () => {
        // A send already out lands first (it may restart for the selection now), then the selection now goes.
        while (inFlight !== null) {
          // eslint-disable-next-line no-await-in-loop -- one send at a time, by design: each waits for the one before
          await inFlight.done
        }
        if (key !== sent) await start(key)
      })()
    },
  }
}
