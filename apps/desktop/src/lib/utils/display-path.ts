/**
 * The path a person reads: a same-named twin's PICK taken out.
 *
 * A device can hold two items with one name in a folder, so each one's row path
 * carries a pick segment (NUL plus a handle) right before the shared name:
 * `/DCIM/\u0000 4242/IMG_1.JPG` without the space. The backend needs it to reach
 * the right item, so `FileEntry.path` keeps it; a person never sees it. Every
 * place that turns a path, or text holding one, into something shown or copied
 * goes through here. The rule is `cmdr_fs::twin_path::shown` in Rust, and the
 * two stay in step: each pick segment goes, with the `/` that joined it.
 */

const PICK = '\u0000'

/** NUL plus a decimal id, then a `/` or the end: exactly what a pick looks like. */
// eslint-disable-next-line no-control-regex -- the NUL is the pick marker this matches, on purpose
const PICK_AT_START = /^\u0000\d+(?=\/|$)/

/**
 * `text` with every pick segment taken out. Any other NUL stays (`file-row-a11y.ts`
 * splits a translated phrase on a bare one). Text without a NUL comes back as is.
 */
export function displayPath(text: string): string {
  if (!text.includes(PICK)) return text
  let out = ''
  let rest = text
  for (let start = rest.indexOf(PICK); start !== -1; start = rest.indexOf(PICK)) {
    const before = rest.slice(0, start)
    const fromPick = rest.slice(start)
    const pick = PICK_AT_START.exec(fromPick)
    if (pick === null) {
      out += rest.slice(0, start + 1)
      rest = fromPick.slice(1)
      continue
    }
    const end = pick[0].length
    if (before.endsWith('/')) {
      // `…/␀4242/name` and a trailing `…/␀4242` both lose the slash before.
      out += before.slice(0, -1)
      rest = fromPick.slice(end)
    } else {
      // A pick that starts the text (a relative row path) loses the one after.
      out += before
      const after = fromPick.slice(end)
      rest = after.startsWith('/') ? after.slice(1) : after
    }
  }
  return out + rest
}
