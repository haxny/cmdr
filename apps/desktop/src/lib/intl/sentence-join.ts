/**
 * Joins whole catalog sentences into one run of text (a tooltip, a toast body)
 * the way the UI language spaces sentences: a space in English, nothing after
 * Chinese's full-width `。`. The separator lives in the catalog
 * (`common.sentenceJoin`), so each locale owns it and no caller hand-joins with
 * `' '`. An empty list answers an empty string.
 */

import { tString } from './messages.svelte'

export function joinSentences(sentences: readonly string[]): string {
  if (sentences.length === 0) return ''
  return sentences.reduce((first, second) => tString('common.sentenceJoin', { first, second }))
}
