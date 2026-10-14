/** Sentence joining follows the UI language's own spacing between sentences. */
import { afterEach, describe, expect, it } from 'vitest'
import { _clearCompiledCacheForTests, setLocale } from './messages.svelte'
import { _setLocaleForTests } from './locale'
import { joinSentences } from './sentence-join'

afterEach(() => {
  setLocale(null)
  _setLocaleForTests(null)
  _clearCompiledCacheForTests()
  document.documentElement.lang = 'en'
})

describe('joinSentences', () => {
  it('answers an empty string for no sentences and the sentence itself for one', () => {
    expect(joinSentences([])).toBe('')
    expect(joinSentences(['On branch main.'])).toBe('On branch main.')
  })

  it('separates English sentences with a space', () => {
    expect(joinSentences(['On branch main.', '1 commit ahead.', 'Dirty.'])).toBe(
      'On branch main. 1 commit ahead. Dirty.',
    )
  })

  it('runs Chinese sentences together after the full-width stop', () => {
    setLocale('zh')
    expect(joinSentences(['位于分支“main”。', '工作树中有未提交的改动。'])).toBe(
      '位于分支“main”。工作树中有未提交的改动。',
    )
    setLocale('zh-Hant')
    expect(joinSentences(['位於分支 main。', '有改動。'])).toBe('位於分支 main。有改動。')
  })
})
