/** What the Tracks filter's count says: "12 of 98 tracks", then ", 3 selected tracks hidden" when it hides some. */
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'

export function filterCountText(shown: number, total: number, hidden: number): string {
  const count = tString('multiRename.tags.filter.count', {
    total,
    shownText: formatInteger(shown),
    totalText: formatInteger(total),
  })
  if (hidden === 0) return count
  const hiddenText = tString('multiRename.tags.filter.hiddenSelected', {
    count: hidden,
    countText: formatInteger(hidden),
  })
  return tString('multiRename.tags.filter.countWithHidden', { count, hidden: hiddenText })
}
