import { describe, it, expect } from 'vitest'
import type { TransferTreeAncestor, TransferTreeRow } from '#lib/tauri-commands/index.js'
import { MAX_PINNED, pinnedHeaders } from './pinned-headers'

/** A flat tree from `'  name'` lines: two spaces per level, a trailing `/` marks a folder. */
function tree(lines: string[]): TransferTreeRow[] {
  return lines.map((line, id) => {
    const depth = (line.length - line.trimStart().length) / 2
    const name = line.trim()
    const folder = name.endsWith('/')
    return {
      id,
      depth,
      label: folder ? name.slice(0, -1) : name,
      kind: folder ? 'folder' : 'file',
      state: 'waiting',
      expanded: folder,
      hasChildren: folder,
      posInSet: 1,
      setSize: 1,
      filesDone: 0,
      filesTotal: 1,
      bytesDone: 0,
      bytesTotal: 1,
      skipped: 0,
      failed: 0,
      failure: null,
    }
  })
}

function ancestorsIn(rows: TransferTreeRow[]): (index: number) => TransferTreeAncestor[] {
  return (index) => {
    const out: TransferTreeAncestor[] = []
    const row = rows.at(index)
    if (!row) return out
    for (let at = index - 1, depth = row.depth - 1; at >= 0 && depth >= 0; at--) {
      if (rows[at].depth === depth) {
        out.unshift({ index: at, row: rows[at] })
        depth--
      }
    }
    return out
  }
}

function pins(rows: TransferTreeRow[], top: number): string[] {
  return pinnedHeaders(top, rows.length, ancestorsIn(rows)).map(
    (header) => `${'  '.repeat(header.row.depth)}${header.row.label}@${String(header.index)}`,
  )
}

const files = (depth: number, count: number): string[] =>
  Array.from({ length: count }, (_, i) => `${'  '.repeat(depth)}f${String(i)}`)

describe('pinnedHeaders', () => {
  it('pins nothing while the folders are in sight', () => {
    const rows = tree(['a/', ...files(1, 20)])
    expect(pins(rows, 0)).toEqual([])
  })

  it('pins a folder once its row scrolled away', () => {
    const rows = tree(['a/', ...files(1, 20)])
    expect(pins(rows, 5)).toEqual(['a@0'])
  })

  it('stacks the open folders by depth', () => {
    const rows = tree(['a/', '  b/', ...files(2, 20)])
    expect(pins(rows, 6)).toEqual(['a@0', '  b@1'])
  })

  it('compacts the folders past the cap into the last header', () => {
    const rows = tree(['a/', '  b/', '    c/', '      d/', '        e/', ...files(5, 20)])
    expect(MAX_PINNED).toBe(3)
    expect(pins(rows, 8)).toEqual(['a@0', '  b@1', '    c/d/e@4'])
  })

  it('keeps the row under the stack’s parent in sight while descending a deep chain', () => {
    const rows = tree(['a/', '  b/', '    c/', '      d/', '        e/', ...files(5, 20)])
    // Rows 1–3 hide under the stack; `e` (row 4) shows right under it.
    expect(pins(rows, 1)).toEqual(['a@0', '  b@1', '    c/d@3'])
  })

  it('drops a folder whose last rows are passing under the stack', () => {
    const rows = tree(['a/', ...files(1, 10), 'b/', ...files(1, 10)])
    expect(pins(rows, 9)).toEqual([])
    expect(pins(rows, 8)).toEqual(['a@0'])
    expect(pins(rows, 12)).toEqual(['b@11'])
  })

  it('pins nothing the window hasn’t loaded', () => {
    const rows = tree(['a/', ...files(1, 20)])
    expect(pinnedHeaders(5, rows.length, () => [])).toEqual([])
  })
})
