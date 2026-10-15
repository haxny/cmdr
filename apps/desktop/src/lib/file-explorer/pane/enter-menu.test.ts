import { describe, expect, it } from 'vitest'
import { buildEnterMenuSections, enterMenuAnchor, enterMenuHighlight } from './enter-menu'

describe('buildEnterMenuSections', () => {
  it('returns one unheaded section with the three rows in order: browse, open, configure', () => {
    const sections = buildEnterMenuSections()
    expect(sections).toHaveLength(1)
    expect(sections[0].heading).toBeUndefined()
    expect(sections[0].items.map((i) => i.value)).toEqual(['browse', 'open', 'configure'])
    // Labels come from the catalog; assert they're non-empty (English fallback).
    for (const item of sections[0].items) expect(item.label.length).toBeGreaterThan(0)
  })
})

describe('enterMenuHighlight', () => {
  it('leads with Open only when the resolved action is open', () => {
    expect(enterMenuHighlight('open')).toBe('open')
  })

  it('leads with Browse for browse and for ask (no preference)', () => {
    expect(enterMenuHighlight('browse')).toBe('browse')
    expect(enterMenuHighlight('ask')).toBe('browse')
  })
})

describe('enterMenuAnchor', () => {
  it('falls back to the origin when there is no pane element', () => {
    expect(enterMenuAnchor(null)).toEqual({ x: 0, y: 0 })
  })

  it('anchors just below the left edge of the cursor row', () => {
    const row = { getBoundingClientRect: () => ({ left: 100, bottom: 40 }) }
    const paneEl = { querySelector: () => row } as unknown as HTMLElement
    expect(enterMenuAnchor(paneEl)).toEqual({ x: 116, y: 40 })
  })

  // `aria-selected` on a file row means MARKED, so a marked row above the cursor must not
  // pull the popup up to itself.
  it('finds the cursor row by its cursor class, never by a marked row', () => {
    const paneEl = document.createElement('div')
    paneEl.innerHTML = `
      <div class="file-entry" role="option" aria-selected="true">marked.txt</div>
      <div class="file-entry is-under-cursor" role="option" aria-selected="false">cursor.zip</div>`
    const [marked, cursor] = paneEl.querySelectorAll('.file-entry')
    marked.getBoundingClientRect = () => ({ left: 0, bottom: 20 }) as DOMRect
    cursor.getBoundingClientRect = () => ({ left: 0, bottom: 40 }) as DOMRect
    expect(enterMenuAnchor(paneEl)).toEqual({ x: 16, y: 40 })
  })

  it('falls back to the pane center when no cursor row is found', () => {
    const paneEl = {
      querySelector: () => null,
      getBoundingClientRect: () => ({ left: 10, top: 20, width: 200, height: 100 }),
    } as unknown as HTMLElement
    expect(enterMenuAnchor(paneEl)).toEqual({ x: 110, y: 70 })
  })
})
