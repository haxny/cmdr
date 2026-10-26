/**
 * One tool line as it renders: the label naming its subject, each path a link that shows it in a
 * pane and then hands focus back to the panes, the "same as above" tag, and what came back. Plus
 * the turn's stats line under the bubble.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick } from 'svelte'
import type { ToolSubject, TurnStats } from '#lib/ipc/bindings.js'
import type { RailToolCall } from './ask-cmdr-messages'
import type { PathTarget } from './ask-cmdr-tool-line'

const { returnFocusToPane } = vi.hoisted(() => ({ returnFocusToPane: vi.fn() }))
vi.mock('./ask-cmdr-trigger.svelte', () => ({ returnFocusToPane }))

import AskCmdrToolLine from './AskCmdrToolLine.svelte'
import AskCmdrTurnStats from './AskCmdrTurnStats.svelte'

function call(subject: ToolSubject | null, overrides: Partial<RailToolCall> = {}): RailToolCall {
  return {
    callId: 'c1',
    tool: 'list_dir',
    running: false,
    subject,
    outcome: { kind: 'items', returned: 3, total: 3, lowerBound: false },
    repeatOf: null,
    ...overrides,
  }
}

function render(tool: RailToolCall): { target: HTMLElement; revealPath: ReturnType<typeof vi.fn> } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const revealPath = vi.fn((_target: PathTarget) => Promise.resolve())
  mount(AskCmdrToolLine, {
    target,
    props: { line: { key: `7:${tool.callId}`, messageId: 7, occurrence: 0, call: tool }, revealPath },
  })
  flushSync()
  return { target, revealPath }
}

function links(target: HTMLElement): HTMLButtonElement[] {
  return [...target.querySelectorAll<HTMLButtonElement>('button.link-button')]
}

describe('AskCmdrToolLine', () => {
  beforeEach(() => {
    returnFocusToPane.mockReset()
  })

  it('names the folder and what came back, on one row', () => {
    const { target } = render(call({ kind: 'folder', path: '/Users/me/Downloads' }))
    expect(target.querySelector('.label')?.textContent.trim()).toBe('Listed Downloads')
    expect(target.querySelector('.outcome')?.textContent).toBe('3 items')
  })

  it('shows a folder in a pane, then hands focus back to the panes', async () => {
    const { target, revealPath } = render(call({ kind: 'folder', path: '/Users/me/Downloads' }))
    const [link] = links(target)
    expect(link.getAttribute('aria-label')).toBe('Show /Users/me/Downloads in a pane')
    link.click()
    await tick()
    expect(revealPath).toHaveBeenCalledWith({ path: '/Users/me/Downloads', kind: 'folder' })
    expect(returnFocusToPane).toHaveBeenCalledTimes(1)
    expect(revealPath.mock.invocationCallOrder[0]).toBeLessThan(returnFocusToPane.mock.invocationCallOrder[0])
  })

  it('shows a file as a file', async () => {
    const { target, revealPath } = render(
      call({ kind: 'files', firstPath: '/Users/me/notes.txt', count: 2 }, { tool: 'inspect_file' }),
    )
    expect(target.querySelector('.label')?.textContent.trim()).toBe('Looked inside notes.txt and 1 more')
    links(target)[0].click()
    await tick()
    expect(revealPath).toHaveBeenCalledWith({ path: '/Users/me/notes.txt', kind: 'file' })
  })

  it('links each folder a search looked in, in order', async () => {
    const { target, revealPath } = render(
      call(
        { kind: 'search', query: '*.pdf', scopePaths: ['/Users/me/Documents', '/Users/me/Desktop'] },
        {
          tool: 'search',
          outcome: { kind: 'matches', returned: 2, matchCount: 2, lowerBound: false },
        },
      ),
    )
    expect(target.querySelector('.label')?.textContent.trim()).toBe('Searched for “*.pdf” in Documents and Desktop')
    const [first, second] = links(target)
    first.click()
    second.click()
    await tick()
    expect(revealPath.mock.calls).toEqual([
      [{ path: '/Users/me/Documents', kind: 'folder' }],
      [{ path: '/Users/me/Desktop', kind: 'folder' }],
    ])
  })

  it('shows a note path outside the notes folder as plain text, never a link', () => {
    const { target } = render(
      call(
        { kind: 'memoryFile', path: '../x.md', absolute: null },
        {
          tool: 'memory_write',
          outcome: { kind: 'refused', reason: { kind: 'memory', refusal: 'outsideMemory' } },
        },
      ),
    )
    expect(links(target)).toHaveLength(0)
    expect(target.querySelector('.label')?.textContent.trim()).toBe('Tried to save a note to ../x.md')
    expect(target.querySelector('.outcome')?.textContent).toBe('that’s outside its notes folder')
  })

  it('tags a repeat and points the link at the tag', () => {
    const { target } = render(
      call({ kind: 'folder', path: '/a' }, { repeatOf: { messageId: 1, callId: 'fake-call-0' } }),
    )
    const tag = target.querySelector('.repeat')
    expect(tag?.textContent).toBe('Same as above')
    expect(links(target)[0].getAttribute('aria-describedby')).toBe(tag?.id)
  })

  it('says no answer was recorded for a settled call without one', () => {
    const { target } = render(call({ kind: 'folder', path: '/a' }, { outcome: null }))
    expect(target.querySelector('.outcome')?.textContent).toBe('no answer recorded')
  })

  it('is not a live region, so a long turn doesn’t chatter', () => {
    const { target } = render(call({ kind: 'folder', path: '/a' }, { running: true, outcome: null }))
    expect(target.querySelector('[role="status"]')).toBeNull()
    expect(target.querySelector('[aria-busy]')).toBeNull()
    expect(target.querySelector('[aria-live]')).toBeNull()
  })
})

describe('AskCmdrTurnStats', () => {
  function renderStats(stats: TurnStats | null): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AskCmdrTurnStats, { target, props: { stats } })
    flushSync()
    return target
  }

  it('words an answered or stopped turn’s spend, with its time', () => {
    const target = renderStats({ toolCalls: 4, wallMs: 12_300, promptTokens: 2000, completionTokens: 345 })
    expect(target.textContent.trim()).toBe('4 tool calls · 12.3s · 2,345 tokens')
  })

  it('leaves the time out for a turn recorded before it was measured', () => {
    const target = renderStats({ toolCalls: 1, wallMs: null, promptTokens: 9, completionTokens: 1 })
    expect(target.textContent.trim()).toBe('1 tool call · 10 tokens')
  })

  it('shows nothing for a turn with no stats or no tool call', () => {
    expect(renderStats(null).textContent.trim()).toBe('')
    expect(renderStats({ toolCalls: 0, wallMs: 1, promptTokens: 1, completionTokens: 1 }).textContent.trim()).toBe('')
  })
})
