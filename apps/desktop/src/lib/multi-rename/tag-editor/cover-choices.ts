/**
 * The cover section's words and its paste: what a cover is (type, pixel size, size), how many files carry it, and the
 * picture a paste holds. Pure; `CoverPanel` and `CoverViewer` share it.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import type { CoverChoice } from '#lib/ipc/bindings.js'

/** The choice that stands for the files without a cover (`tag_editor::cover_choices::NO_COVER_KEY`). */
export const NO_COVER_KEY = 'none'

/** The largest picture a paste sends: the backend refuses more (`MAX_COVER_BYTES`, 10 MiB). */
// eslint-disable-next-line cmdr/no-private-unit-format -- a fixed byte limit the backend shares, not a size shown as text
export const MAX_COVER_BYTES = 10 * 1024 * 1024

/** A cover opened large: the choice, how many readable files there are, and its `data:` URL (`null`: can't show). */
export interface CoverShown {
  choice: CoverChoice
  total: number
  image: string | null
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
