/**
 * The Album pane over the Tracks pane's selection: the backend sums up only the selected tracks in the view's `shared`
 * rows (`setViewScope`, `scopeCount`), and an Album edit sets only those tracks. Every track while none is selected.
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

/**
 * Keeps the backend's view scope on the selected tracks: one `setViewScope` per selection change, after
 * `VIEW_SCOPE_DELAY_MS` of quiet. Keyed by the paths alone, so a sort (a new order) sends nothing; an emptied or vanished
 * selection widens it to every track (`null`). Call it while a component initializes (it owns an `$effect`).
 */
export function followViewScope(deps: {
  selected: () => ReadonlySet<string>
  setViewScope: (paths: string[] | null) => Promise<void>
}): void {
  const key = $derived(JSON.stringify([...deps.selected()].sort()))
  /** The selection last sent; nothing selected at first, which the backend already shows. */
  let sent = JSON.stringify([])
  $effect(() => {
    const next = key
    if (next === sent) return
    const timer = setTimeout(() => {
      sent = next
      const paths = JSON.parse(next) as string[]
      void deps.setViewScope(paths.length > 0 ? paths : null)
    }, VIEW_SCOPE_DELAY_MS)
    return () => {
      clearTimeout(timer)
    }
  })
}
