/**
 * The media fields as the sheet shows them: each field's words, the field picker's two groups
 * (photos and videos, music) with their filter and order, and how a pick goes into the mask. Pure:
 * it names message keys, and the caller words them.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { MediaField, MediaFieldOverview } from '#lib/ipc/bindings.js'
import { MEDIA_FIELDS } from './media-token'
import { insertAtCaret } from './spec'

/** Each field's name in the picker, a row's reason, and the token editor. */
export const MEDIA_FIELD_LABELS: Readonly<Record<MediaField, MessageKey>> = {
  exifTaken: 'multiRename.mediaField.exifTaken',
  exifCamera: 'multiRename.mediaField.exifCamera',
  exifMake: 'multiRename.mediaField.exifMake',
  exifModel: 'multiRename.mediaField.exifModel',
  exifLens: 'multiRename.mediaField.exifLens',
  exifIso: 'multiRename.mediaField.exifIso',
  exifFocal: 'multiRename.mediaField.exifFocal',
  exifWidth: 'multiRename.mediaField.exifWidth',
  exifHeight: 'multiRename.mediaField.exifHeight',
  exifOrientation: 'multiRename.mediaField.exifOrientation',
  tagTitle: 'multiRename.mediaField.tagTitle',
  tagArtist: 'multiRename.mediaField.tagArtist',
  tagAlbumArtist: 'multiRename.mediaField.tagAlbumArtist',
  tagAlbum: 'multiRename.mediaField.tagAlbum',
  tagTrack: 'multiRename.mediaField.tagTrack',
  tagDisc: 'multiRename.mediaField.tagDisc',
  tagYear: 'multiRename.mediaField.tagYear',
  tagGenre: 'multiRename.mediaField.tagGenre',
  tagComment: 'multiRename.mediaField.tagComment',
}

/** Where a field comes from: the mask name's part before the dot. */
export type MediaSource = 'exif' | 'tag'

/** The group headings name the formats outright, so someone looking for "EXIF" or "ID3" finds them. */
const GROUP_HEADINGS: Readonly<Record<MediaSource, MessageKey>> = {
  exif: 'multiRename.mediaPicker.photos',
  tag: 'multiRename.mediaPicker.music',
}

export function sourceOf(field: MediaField): MediaSource {
  return MEDIA_FIELDS[field].name.startsWith('exif.') ? 'exif' : 'tag'
}

/** The mask placeholder for `field` in its default format: `[=exif.taken]`. */
export function fieldToken(field: MediaField): string {
  return `[=${MEDIA_FIELDS[field].name}]`
}

export interface PickerField {
  field: MediaField
  label: MessageKey
  /** The mask name, `exif.taken`. */
  name: string
  /** The value from the first file in rename order that has it, or `null` when none read so far does. */
  example: string | null
}

export interface PickerGroup {
  source: MediaSource
  heading: MessageKey
  fields: PickerField[]
}

/**
 * The picker's groups, the one more of the session's files have first (photos on a tie), each field
 * in `MediaField` order, filtered by `filter` against the field's words, its mask name, and its
 * group's heading, in any case. A group nothing in it matches is left out.
 */
export function pickerGroups(
  overview: readonly MediaFieldOverview[],
  filter: string,
  words: (key: MessageKey) => string,
): PickerGroup[] {
  const needle = filter.trim().toLowerCase()
  const matches = (text: string): boolean => text.toLowerCase().includes(needle)
  const files: Record<MediaSource, number> = { exif: 0, tag: 0 }
  for (const { field, filesWithIt } of overview) files[sourceOf(field)] += filesWithIt
  const order: MediaSource[] = files.tag > files.exif ? ['tag', 'exif'] : ['exif', 'tag']
  return order
    .map((source) => {
      const heading = GROUP_HEADINGS[source]
      const all = overview
        .filter(({ field }) => sourceOf(field) === source)
        .map(({ field, example }) => ({
          field,
          label: MEDIA_FIELD_LABELS[field],
          name: MEDIA_FIELDS[field].name,
          example,
        }))
      const fields =
        needle === '' || matches(words(heading)) ? all : all.filter((f) => matches(words(f.label)) || matches(f.name))
      return { source, heading, fields }
    })
    .filter((group) => group.fields.length > 0)
}

/**
 * The `media ▾` chip's word: its label without the ▾, which the chip draws itself outside the part
 * that truncates, so a long translation never cuts the affordance. The catalog's label carries the ▾
 * for translators' context; one that dropped or moved it reads the same.
 */
export function chipWord(label: string): string {
  return label.replaceAll('▾', '').trim()
}

/** Where focus goes when the picker closes: the name mask, the control after or before the button, or it stays. */
export type PickerExit = 'mask' | 'forward' | 'backward' | 'none'

/** Where the last pick left the mask: its text and the caret after the token. */
export interface Inserted {
  mask: string
  caret: number
}

/**
 * Puts `token` into `mask` at `caret`. Right after the last pick (`after`: the mask and caret
 * unchanged since), a space goes first, so `[=tag.artist] [=tag.title]` reads as two words.
 */
export function insertField({
  mask,
  caret,
  token,
  after,
}: {
  mask: string
  caret: number | null
  token: string
  after: Inserted | null
}): Inserted {
  const follows =
    after !== null && after.mask === mask && after.caret === caret && !mask.slice(0, caret ?? 0).endsWith(' ')
  return insertAtCaret(mask, follows ? ` ${token}` : token, caret)
}
