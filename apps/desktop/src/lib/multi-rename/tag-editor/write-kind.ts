/**
 * The content kind the Tags window writes as (Audiobook or Music), remembered for this user across sheets and restarts
 * in `app-status.json`, as the app remembers other UI choices. Audiobook until the user first picks one.
 */
import { loadTagWriteKind, saveTagWriteKind } from '#lib/app-status-store.js'
import type { ContentKind } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'

export const WRITE_KINDS: readonly ContentKind[] = ['audiobook', 'music']

/** Each kind's name, as the Write dialog's toggle shows it. */
export const KIND_LABELS: Readonly<Record<ContentKind, MessageKey>> = {
  audiobook: 'multiRename.tags.kind.audiobook',
  music: 'multiRename.tags.kind.music',
}

/** The first-run choice: the user's canon is audiobooks. */
export const DEFAULT_WRITE_KIND: ContentKind = 'audiobook'

/** The kind last picked, or the default when none was (or what's stored isn't a kind). */
export async function loadWriteKind(): Promise<ContentKind> {
  const stored = await loadTagWriteKind()
  return WRITE_KINDS.find((kind) => kind === stored) ?? DEFAULT_WRITE_KIND
}

export function saveWriteKind(kind: ContentKind): void {
  void saveTagWriteKind(kind)
}
