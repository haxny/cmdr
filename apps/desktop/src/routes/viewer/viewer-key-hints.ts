import { tString } from '#lib/intl/messages.svelte.js'

/**
 * The status bar's text-mode hint ("W wrap · F tail · ⌘F search") from the keys the user
 * actually has: wrap and tail come from the shortcuts registry, so a rebinding shows here and
 * a key turned off drops its part. Each part is its own message, so a language can order the
 * key and the word its own way.
 */
export function textModeHint(keys: { wrap: string | undefined; tail: string | undefined; search: string }): string {
  const parts = [
    keys.wrap === undefined ? null : tString('viewer.statusBar.hint.wrap', { key: keys.wrap }),
    keys.tail === undefined ? null : tString('viewer.statusBar.hint.tail', { key: keys.tail }),
    tString('viewer.statusBar.hint.search', { key: keys.search }),
  ]
  return parts.filter((part) => part !== null).join(' · ')
}
