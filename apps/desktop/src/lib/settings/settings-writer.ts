/**
 * One window's write path into the shared settings store: which keys it changed,
 * handing them to the plugin store in order, and the debounced disk save.
 *
 * Two steps, on purpose. A change reaches the plugin store at once: the plugin
 * holds the store in Rust memory and writes every loaded store to disk at
 * `RunEvent::Exit`, so from there a quit keeps it. Only the disk write is
 * debounced. ❌ Don't fold the store write into that debounce: a quit inside it
 * dropped the change (#698).
 *
 * What a key's write MEANS (set the value, or delete a reset key) is the
 * caller's `writePatch`; this module only decides when, and in what order.
 */

import type { SettingId } from './types'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('settings')

const SAVE_DEBOUNCE_MS = 500

export interface SettingsWriterDeps {
  /** Writes these keys into the plugin store, without a disk write. May throw partway through. */
  writePatch: (patch: readonly SettingId[]) => Promise<void>
  /** Writes the plugin store to disk. */
  saveToDisk: () => Promise<void>
}

export interface SettingsWriter {
  /** Records a change this window made: hands it to the store now, and schedules the disk save. */
  record: (id: SettingId) => void
  /** Writes everything pending to disk now, even with nothing pending. Answers whether it landed. */
  save: () => Promise<boolean>
  /** Writes this window's pending changes to disk, or nothing if it has none. Answers whether nothing is left unsaved. */
  flush: () => Promise<boolean>
}

export function createSettingsWriter({ writePatch, saveToDisk }: SettingsWriterDeps): SettingsWriter {
  // Ids this window set or reset and hasn't handed to the plugin store yet. Only
  // these, ❌ never the whole ledger: windows share one in-memory plugin store
  // per file, and a window opened inside another's save debounce never hears
  // that window's newest value, so writing its WHOLE ledger would delete or roll
  // back a key it never saw. A change heard from another window stays out: that
  // window persists it. DETAILS § Sparse persistence.
  const changedHere = new Set<SettingId>()
  // Whether this window wrote to the plugin store since its last disk save.
  let unsavedHere = false
  let saveTimeout: ReturnType<typeof setTimeout> | null = null
  // Every store write queues here, so writes land in the order they were made:
  // the plugin's commands are async, and two in flight could finish in either
  // order (a picker preview landing on top of the pick made after it).
  let storeWrites: Promise<void> = Promise.resolve()

  /** Runs `task` once every write queued before it is done, whether or not they succeeded. */
  function queue<T>(task: () => Promise<T>): Promise<T> {
    const run = storeWrites.then(task)
    storeWrites = run.then(
      () => undefined,
      () => undefined,
    )
    return run
  }

  /**
   * Hands the pending changes to the plugin store. Takes them up front, so one
   * made during the awaits gets its own write; a patch that didn't land goes back
   * for the next attempt.
   */
  async function push(): Promise<void> {
    if (changedHere.size === 0) return
    const patch = [...changedHere]
    changedHere.clear()
    try {
      await writePatch(patch)
      unsavedHere = true
    } catch (error) {
      for (const id of patch) changedHere.add(id)
      throw error
    }
  }

  async function pushAndSave(): Promise<void> {
    await push()
    await saveToDisk()
    unsavedHere = false
  }

  /**
   * A failed attempt is retried once, and the retry re-pushes every change that
   * didn't reach the store: an attempt can throw partway through, so re-saving
   * alone would report keys that never got there.
   */
  function saveNow(): Promise<boolean> {
    return queue(async () => {
      try {
        await pushAndSave()
        return true
      } catch (error) {
        log.error('Failed to save settings: {error}', { error })
        try {
          await pushAndSave()
          log.info('Retry save succeeded')
          return true
        } catch (retryError) {
          log.error('Retry save failed: {error}', { error: retryError })
          return false
        }
      }
    })
  }

  function scheduleSave(): void {
    if (saveTimeout) clearTimeout(saveTimeout)
    saveTimeout = setTimeout(() => {
      // Cleared as it fires, not when the save settles: a change during the
      // save's awaits schedules a NEW timer, which a late clear would orphan.
      saveTimeout = null
      void saveNow()
    }, SAVE_DEBOUNCE_MS)
  }

  function cancelScheduledSave(): void {
    if (saveTimeout) {
      clearTimeout(saveTimeout)
      saveTimeout = null
    }
  }

  return {
    record(id) {
      changedHere.add(id)
      void queue(push).catch((error: unknown) => {
        // Still pending; the next save retries it.
        log.warn('Could not hand a settings change to the store yet: {error}', { error })
      })
      scheduleSave()
    },
    save() {
      cancelScheduledSave()
      return saveNow()
    },
    flush() {
      if (changedHere.size === 0 && !unsavedHere) return Promise.resolve(true)
      cancelScheduledSave()
      return saveNow()
    },
  }
}
