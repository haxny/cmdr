/**
 * The words a tool line says: its label with the subject in it, what came back, and the turn's
 * stats line. English, the source catalog.
 */

import { describe, expect, it } from 'vitest'
import type { MemoryRefusalKind, RefusalReason, ToolOutcome, ToolSubject } from '#lib/ipc/bindings.js'
import { lineLabelText, toolLineLabel, toolOutcomeText, turnStatsText } from './ask-cmdr-labels'
import type { RailToolCall } from './ask-cmdr-messages'

function call(tool: string, subject: ToolSubject | null, overrides: Partial<RailToolCall> = {}): RailToolCall {
  return {
    callId: 'c1',
    tool,
    running: false,
    subject,
    outcome: { kind: 'answered' },
    repeatOf: null,
    ...overrides,
  }
}

function said(tool: string, subject: ToolSubject | null, overrides: Partial<RailToolCall> = {}): string {
  return lineLabelText(toolLineLabel(call(tool, subject, overrides)))
}

describe('toolLineLabel', () => {
  it('names the folder a listing looked at, running and done, as a place to go', () => {
    const folder: ToolSubject = { kind: 'folder', path: '/Users/me/Downloads' }
    expect(said('list_dir', folder, { running: true, outcome: null })).toBe('Listing Downloads')
    expect(said('list_dir', folder)).toBe('Listed Downloads')
    expect(toolLineLabel(call('list_dir', folder)).place).toEqual({
      path: '/Users/me/Downloads',
      kind: 'folder',
      shown: 'Downloads',
      linked: true,
    })
    expect(said('folder_importance', folder)).toBe('Checked how important Downloads is')
  })

  it('shows the root as itself, since it has no last segment', () => {
    expect(said('list_dir', { kind: 'folder', path: '/' })).toBe('Listed /')
  })

  it('falls back to today’s words when the subject is unknown or unreadable', () => {
    expect(said('list_dir', null)).toBe('Listed a folder')
    expect(said('list_dir', { kind: 'unparsed' }, { running: true, outcome: null })).toBe('Listing a folder')
    expect(said('search', { kind: 'unparsed' })).toBe('Searched your files')
  })

  it('shows "Working" for a tool the rail has no label for', () => {
    expect(said('some_new_tool', null, { running: true, outcome: null })).toBe('Working')
    expect(said('some_new_tool', null)).toBe('Used a tool')
  })

  it('names the pane’s folder once the call says which, and whether it read the selection', () => {
    expect(said('list_pane_files', { kind: 'focusedPane', folder: null, selectionOnly: null })).toBe('Listed a folder')
    expect(said('list_pane_files', { kind: 'focusedPane', folder: '/Users/me/Desktop', selectionOnly: false })).toBe(
      'Listed Desktop',
    )
    expect(said('list_pane_files', { kind: 'focusedPane', folder: '/Users/me/Desktop', selectionOnly: true })).toBe(
      'Listed your selection in Desktop',
    )
  })

  it('names the first file, and how many more the call asked about', () => {
    expect(said('inspect_file', { kind: 'files', firstPath: '/a/notes.txt', count: 1 })).toBe('Looked inside notes.txt')
    expect(said('inspect_file', { kind: 'files', firstPath: '/a/notes.txt', count: 3 })).toBe(
      'Looked inside notes.txt and 2 more',
    )
    expect(said('image_facts', { kind: 'files', firstPath: '/a/IMG_1.jpg', count: 1 })).toBe('Read what’s in IMG_1.jpg')
    expect(
      toolLineLabel(call('inspect_file', { kind: 'files', firstPath: '/a/notes.txt', count: 1 })).place?.kind,
    ).toBe('file')
  })

  it('words a search by what it has: a pattern, folders to look in, both, or neither', () => {
    const search = (query: string | null, scopePaths: string[]): string =>
      said('search', { kind: 'search', query, scopePaths })
    expect(search('*.pdf', ['/Users/me/Documents'])).toBe('Searched for “*.pdf” in Documents')
    expect(search('*.pdf', ['/Users/me/Documents', '/Users/me/Desktop'])).toBe(
      'Searched for “*.pdf” in Documents and Desktop',
    )
    expect(search('*.pdf', [])).toBe('Searched for “*.pdf”')
    expect(search(null, ['/Users/me/Documents'])).toBe('Searched in Documents')
    expect(search(null, [])).toBe('Searched your files')
    const label = toolLineLabel(call('search', { kind: 'search', query: 'x', scopePaths: ['/a', '/b'] }))
    expect(label.places.map((place) => place.path)).toEqual(['/a', '/b'])
    expect(label.places.every((place) => place.kind === 'folder' && place.linked)).toBe(true)
  })

  it('passes a search’s folder count, so a case language can tell one folder from several', () => {
    const label = (query: string | null, scopePaths: string[]) =>
      toolLineLabel(call('search', { kind: 'search', query, scopePaths }))
    expect(label('x', ['/a', '/b'])).toMatchObject({
      key: 'askCmdr.tool.search.queryInFolders.done',
      params: { folders: 2 },
    })
    expect(label(null, ['/a'])).toMatchObject({ key: 'askCmdr.tool.search.inFolders.done', params: { folders: 1 } })
  })

  it('names a photo search’s words, and counts what a plan carries', () => {
    expect(said('search_photos', { kind: 'photoSearch', query: 'beach' })).toBe('Searched your photos for “beach”')
    expect(said('propose_rename_plan', { kind: 'renamePlan', count: 1 })).toBe('Prepared a plan to rename 1 file')
    expect(said('propose_rename_plan', { kind: 'renamePlan', count: 12 })).toBe('Prepared a plan to rename 12 files')
    expect(said('propose_suggestions', { kind: 'suggestionSweep', count: 2 })).toBe('Prepared 2 suggestions')
  })

  it('links a note only when it lands inside the notes folder', () => {
    const inside: ToolSubject = { kind: 'memoryFile', path: 'prefs.md', absolute: '/data/memory/prefs.md' }
    const outside: ToolSubject = { kind: 'memoryFile', path: '../etc/x.md', absolute: null }
    expect(said('memory_write', inside)).toBe('Saved a note to prefs.md')
    expect(said('memory_edit', inside)).toBe('Updated prefs.md')
    expect(toolLineLabel(call('memory_write', inside)).place).toEqual({
      path: '/data/memory/prefs.md',
      kind: 'file',
      shown: 'prefs.md',
      linked: true,
    })
    expect(toolLineLabel(call('memory_write', outside)).place?.linked).toBe(false)
    expect(toolLineLabel(call('memory_write', outside)).place?.shown).toBe('../etc/x.md')
  })

  it('never claims a refused note was saved', () => {
    const refused: ToolOutcome = { kind: 'refused', reason: { kind: 'memory', refusal: 'outsideMemory' } }
    const inside: ToolSubject = { kind: 'memoryFile', path: 'prefs.md', absolute: '/data/memory/prefs.md' }
    expect(said('memory_write', inside, { outcome: refused })).toBe('Tried to save a note to prefs.md')
    expect(said('memory_edit', inside, { outcome: refused })).toBe('Tried to update prefs.md')
    expect(said('memory_write', null, { outcome: refused })).toBe('Tried to save a note')
    expect(said('memory_edit', null, { outcome: refused })).toBe('Tried to update its notes')
  })

  it('names what a call outside the agent’s view asked for, and keeps the generic words for a nameless one', () => {
    expect(said('delete', { kind: 'unknownTool', name: 'delete' })).toBe('Asked for “delete”')
    expect(said('', { kind: 'unknownTool', name: '' })).toBe('Used a tool')
    expect(said('', { kind: 'unknownTool', name: '' }, { running: true, outcome: null })).toBe('Working')
  })

  /** Exhaustive by type: a new subject kind fails to compile here until it has a sample. */
  it('says something for every subject kind, running and done', () => {
    const samples: Record<ToolSubject['kind'], [string, ToolSubject]> = {
      panes: ['app_state', { kind: 'panes' }],
      focusedPane: ['list_pane_files', { kind: 'focusedPane', folder: '/a', selectionOnly: false }],
      folder: ['list_dir', { kind: 'folder', path: '/a' }],
      files: ['inspect_file', { kind: 'files', firstPath: '/a/b', count: 1 }],
      search: ['search', { kind: 'search', query: 'x', scopePaths: [] }],
      photoSearch: ['search_photos', { kind: 'photoSearch', query: 'x' }],
      importantFolders: ['important_folders', { kind: 'importantFolders' }],
      drives: ['list_volumes', { kind: 'drives' }],
      operationLog: ['operations_list', { kind: 'operationLog' }],
      operation: ['operations_get', { kind: 'operation', id: 'op-1' }],
      renamePlan: ['propose_rename_plan', { kind: 'renamePlan', count: 2 }],
      suggestions: ['list_suggestions', { kind: 'suggestions' }],
      suggestionGroup: ['get_suggestion_group', { kind: 'suggestionGroup' }],
      suggestionSweep: ['propose_suggestions', { kind: 'suggestionSweep', count: 2 }],
      nothingToSuggest: ['nothing_to_suggest', { kind: 'nothingToSuggest' }],
      memoryFile: ['memory_write', { kind: 'memoryFile', path: 'a.md', absolute: null }],
      unknownTool: ['nope', { kind: 'unknownTool', name: 'nope' }],
      unparsed: ['list_dir', { kind: 'unparsed' }],
    }
    for (const [tool, subject] of Object.values(samples)) {
      expect(said(tool, subject, { running: true, outcome: null }).length).toBeGreaterThan(0)
      expect(said(tool, subject).length).toBeGreaterThan(0)
    }
  })
})

describe('toolOutcomeText', () => {
  const settled = (outcome: ToolOutcome | null): string | null =>
    toolOutcomeText(call('list_dir', null, { outcome, running: false }))

  it('counts items, and says how many of how many when the model saw part of a list', () => {
    expect(settled({ kind: 'items', returned: 1, total: 1, lowerBound: false })).toBe('1 item')
    expect(settled({ kind: 'items', returned: 42, total: null, lowerBound: false })).toBe('42 items')
    expect(settled({ kind: 'items', returned: 200, total: 4812, lowerBound: false })).toBe('showed 200 of 4,812')
    expect(settled({ kind: 'items', returned: 200, total: 4812, lowerBound: true })).toBe(
      'showed 200 of at least 4,812',
    )
  })

  it('counts matches, and never says "none" while the search was still covering ground', () => {
    expect(settled({ kind: 'matches', returned: 20, matchCount: 1234, lowerBound: false })).toBe('1,234 matches')
    expect(settled({ kind: 'matches', returned: 1, matchCount: 1, lowerBound: true })).toBe('1 match so far')
    expect(settled({ kind: 'nothingFound', lowerBound: false })).toBe('nothing found')
    expect(settled({ kind: 'nothingFound', lowerBound: true })).toBe('nothing found yet')
  })

  it('says nothing more for a plain answer or a saved note: the label already said it', () => {
    expect(settled({ kind: 'answered' })).toBeNull()
    expect(settled({ kind: 'saved' })).toBeNull()
  })

  it('says nothing more when the count only repeats the paths the label names, and keeps a partial answer', () => {
    const files: ToolSubject = { kind: 'files', firstPath: '/a/notes.txt', count: 2 }
    const outcomeOf = (tool: string, outcome: ToolOutcome): string | null =>
      toolOutcomeText(call(tool, files, { outcome }))
    const all: ToolOutcome = { kind: 'items', returned: 2, total: 2, lowerBound: false }
    expect(outcomeOf('inspect_file', all)).toBeNull()
    expect(outcomeOf('image_facts', all)).toBeNull()
    expect(outcomeOf('inspect_file', { kind: 'items', returned: 1, total: 2, lowerBound: false })).toBe('showed 1 of 2')
    // A folder's count is news: the label names the folder, not how much it holds.
    expect(toolOutcomeText(call('list_dir', { kind: 'folder', path: '/a' }, { outcome: all }))).toBe('2 items')
  })

  it('says nothing while the call runs, and that no answer was recorded for a settled one without', () => {
    expect(toolOutcomeText(call('list_dir', null, { running: true, outcome: null }))).toBeNull()
    expect(settled(null)).toBe('no answer recorded')
  })

  /** Exhaustive by type: a new outcome kind or refusal reason fails to compile until it has copy. */
  it('has words for every outcome, refusal reason, and memory refusal', () => {
    const outcomes: Record<ToolOutcome['kind'], ToolOutcome[]> = {
      items: [{ kind: 'items', returned: 3, total: 3, lowerBound: false }],
      matches: [{ kind: 'matches', returned: 3, matchCount: 3, lowerBound: false }],
      nothingFound: [{ kind: 'nothingFound', lowerBound: false }],
      answered: [],
      indexUnavailable: [
        { kind: 'indexUnavailable', reason: 'imageIndexingOff' },
        { kind: 'indexUnavailable', reason: 'semanticModelNotInstalled' },
      ],
      staged: [{ kind: 'staged', count: 4 }],
      notStaged: [{ kind: 'notStaged' }],
      saved: [],
      refused: [],
    }
    const reasons: Record<Exclude<RefusalReason['kind'], 'memory'>, RefusalReason> = {
      toolUnavailable: { kind: 'toolUnavailable' },
      repeatedCall: { kind: 'repeatedCall' },
      problem: { kind: 'problem' },
    }
    const memory: Record<MemoryRefusalKind, true> = {
      outsideMemory: true,
      notMarkdown: true,
      noPath: true,
      directoryFull: true,
      noSuchFile: true,
      noMatch: true,
      notUnique: true,
      unwritable: true,
      unavailable: true,
      other: true,
    }
    const all: ToolOutcome[] = [
      ...Object.values(outcomes).flat(),
      ...Object.values(reasons).map((reason): ToolOutcome => ({ kind: 'refused', reason })),
      ...(Object.keys(memory) as MemoryRefusalKind[]).map((refusal): ToolOutcome => ({
        kind: 'refused',
        reason: { kind: 'memory', refusal },
      })),
    ]
    const texts = all.map((outcome) => settled(outcome))
    for (const text of texts) expect(text?.length ?? 0).toBeGreaterThan(0)
    // Each refusal says which one it was.
    expect(new Set(texts).size).toBe(texts.length)
  })
})

describe('turnStatsText', () => {
  it('words what a turn spent: calls, time, and tokens', () => {
    expect(turnStatsText({ toolCalls: 3, wallMs: 4200, promptTokens: 1000, completionTokens: 234 })).toBe(
      '3 tool calls · 4.2s · 1,234 tokens',
    )
    expect(turnStatsText({ toolCalls: 1, wallMs: 800, promptTokens: 1, completionTokens: 0 })).toBe(
      '1 tool call · 800ms · 1 token',
    )
  })

  it('leaves the time out for a turn recorded before it was measured', () => {
    expect(turnStatsText({ toolCalls: 2, wallMs: null, promptTokens: 10, completionTokens: 5 })).toBe(
      '2 tool calls · 15 tokens',
    )
  })

  it('leads with "Stopped" for a turn the user stopped', () => {
    expect(turnStatsText({ toolCalls: 2, wallMs: 4200, promptTokens: 10, completionTokens: 5, stopped: true })).toBe(
      'Stopped · 2 tool calls · 4.2s · 15 tokens',
    )
    expect(turnStatsText({ toolCalls: 2, wallMs: 4200, promptTokens: 10, completionTokens: 5, stopped: false })).toBe(
      '2 tool calls · 4.2s · 15 tokens',
    )
  })

  it('says nothing for a turn that ran no tool, or has no stats', () => {
    expect(turnStatsText(null)).toBeNull()
    expect(turnStatsText({ toolCalls: 0, wallMs: 10, promptTokens: 1, completionTokens: 1 })).toBeNull()
  })
})
