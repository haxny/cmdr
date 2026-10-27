/** The words for each tag field, in the editor's display order (`TagField::ALL`). */

import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { TagField } from '#lib/ipc/bindings.js'

export const FIELD_LABELS: Readonly<Record<TagField, MessageKey>> = {
  title: 'multiRename.tags.field.title',
  artist: 'multiRename.tags.field.artist',
  albumArtist: 'multiRename.tags.field.albumArtist',
  album: 'multiRename.tags.field.album',
  performer: 'multiRename.tags.field.performer',
  composer: 'multiRename.tags.field.composer',
  date: 'multiRename.tags.field.date',
  genre: 'multiRename.tags.field.genre',
  publisher: 'multiRename.tags.field.publisher',
  translator: 'multiRename.tags.field.translator',
  track: 'multiRename.tags.field.track',
  trackTotal: 'multiRename.tags.field.trackTotal',
  disc: 'multiRename.tags.field.disc',
  discTotal: 'multiRename.tags.field.discTotal',
  comment: 'multiRename.tags.field.comment',
  description: 'multiRename.tags.field.description',
  www: 'multiRename.tags.field.www',
}

/** Every field, in display order. */
export const TAG_FIELDS = Object.keys(FIELD_LABELS) as readonly TagField[]
