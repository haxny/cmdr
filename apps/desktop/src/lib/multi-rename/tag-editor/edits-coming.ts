/**
 * Album and clipboard edits started but not yet handed to the state (a paste reading the clipboard, a commit waiting
 * for the pane's scope): ⌘⏎ and ⌘Z wait for them (`sent`), so the edit just made is the one they see. `TagsContent`
 * holds one.
 */
import { SETTLE_LIMIT_MS } from './tag-editor-state.svelte'

export interface EditsComing {
  /** Notes `run` as an edit on its way and hands it back. */
  track<T>(run: Promise<T>): Promise<T>
  /** Resolves `true` once every edit on its way has settled, `false` when one is still out after `SETTLE_LIMIT_MS`. */
  sent(): Promise<boolean>
}

export function createEditsComing(): EditsComing {
  // Bookkeeping of pending promises, nothing renders from it.
  const coming = new Set<Promise<unknown>>()
  return {
    track(run) {
      coming.add(run)
      void run.then(
        () => coming.delete(run),
        () => coming.delete(run),
      )
      return run
    },
    async sent() {
      let timer: ReturnType<typeof setTimeout> | undefined
      const gaveUp = new Promise<false>((resolve) => {
        timer = setTimeout(() => {
          resolve(false)
        }, SETTLE_LIMIT_MS)
      })
      const sent = (async () => {
        while (coming.size > 0) await Promise.allSettled([...coming])
        return true as const
      })()
      try {
        return await Promise.race([sent, gaveUp])
      } finally {
        clearTimeout(timer)
      }
    },
  }
}
