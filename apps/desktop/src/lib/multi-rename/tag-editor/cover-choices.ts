/**
 * The cover section's words and its paste: what a cover is (type, pixel size, size), how many files carry it, and the
 * picture a paste holds. Pure; `CoverPanel` and `CoverViewer` share it.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { formatInteger } from '#lib/intl/number-format.js'
import type { CoverChoice } from '#lib/ipc/bindings.js'

/** The choice that stands for the files without a cover (`tag_editor::cover_choices::NO_COVER_KEY`). */
export const NO_COVER_KEY = 'none'

/** The largest picture a paste sends: the backend refuses more (`MAX_COVER_BYTES`, 10 MiB). */
// eslint-disable-next-line cmdr/no-private-unit-format -- a fixed byte limit the backend shares, not a size shown as text
export const MAX_COVER_BYTES = 10 * 1024 * 1024

/**
 * The largest pasted TIFF sent (`MAX_PASTED_TIFF_BYTES`, 40 MiB): the backend converts it, and an uncompressed clipboard
 * TIFF is far bigger than the PNG or JPEG it becomes.
 */
// eslint-disable-next-line cmdr/no-private-unit-format -- a fixed byte limit the backend shares, not a size shown as text
export const MAX_PASTED_TIFF_BYTES = 40 * 1024 * 1024

/** How many bytes a pasted picture of `type` may have before it's refused here, unsent. */
export function pasteLimit(type: string): number {
  return type === 'image/tiff' ? MAX_PASTED_TIFF_BYTES : MAX_COVER_BYTES
}

/** A cover opened large: the choice, and how many readable files there are. The viewer fetches the full image. */
export interface CoverShown {
  choice: CoverChoice
  total: number
}

/** The part of `rect` inside `clip` (the scroller the section sits in); `null` when nothing of it shows. */
export function visiblePart(rect: DOMRect, clip: DOMRect | null): DOMRect | null {
  if (clip === null) return rect
  const left = Math.max(rect.left, clip.left)
  const top = Math.max(rect.top, clip.top)
  const right = Math.min(rect.right, clip.right)
  const bottom = Math.min(rect.bottom, clip.bottom)
  return right > left && bottom > top ? new DOMRect(left, top, right - left, bottom - top) : null
}

/** "image/jpeg, 600×400 px, 2.00 KiB", or without the pixels when the header couldn't be read. */
export function coverDetails(choice: CoverChoice, sizeText: string): string {
  if (choice.width !== null && choice.height !== null) {
    return tString('multiRename.tags.cover.detailsPixels', {
      mime: choice.mime,
      width: choice.width,
      height: choice.height,
      sizeText,
    })
  }
  return tString('multiRename.tags.cover.details', { mime: choice.mime, sizeText })
}

/** "12 files". */
export function filesText(count: number): string {
  return tString('multiRename.tags.cover.files', { count, countText: formatInteger(count) })
}

/** The first picture a paste holds (an `image/*` file item), or `null`. */
export function pastedPicture(data: Pick<DataTransfer, 'items'> | null): File | null {
  for (const item of data?.items ?? []) {
    if (item.kind !== 'file' || !item.type.startsWith('image/')) continue
    const file = item.getAsFile()
    if (file !== null) return file
  }
  return null
}

/** What each failed cover pick, paste or drop says (`TagEditorError` kinds about covers). */
export const COVER_ERROR_KEYS: Readonly<Partial<Record<string, MessageKey>>> = {
  coverTooLarge: 'multiRename.tags.cover.tooLarge',
  badCoverName: 'multiRename.tags.cover.unreadable',
  coverUnreadable: 'multiRename.tags.cover.unreadable',
  coverNotAnImage: 'multiRename.tags.cover.notAnImage',
  pastedNotAnImage: 'multiRename.tags.cover.pasteNotAnImage',
  coverTimedOut: 'multiRename.tags.cover.timedOut',
  coverBusy: 'multiRename.tags.cover.busy',
  pasteTimedOut: 'multiRename.tags.cover.pasteTimedOut',
  coverNotAFile: 'multiRename.tags.cover.notAFile',
  unknownCover: 'multiRename.tags.cover.gone',
}
