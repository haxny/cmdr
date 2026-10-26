/**
 * The pure history fold: a loaded conversation becomes rail items, each tool line reading the
 * outcome and subject the backend paired onto its call.
 */

import { describe, expect, it } from 'vitest'
import { buildRailMessages } from './ask-cmdr-history'
import type { ConversationDetailView } from '#lib/tauri-commands/index.js'

function detail(messages: ConversationDetailView['messages']): ConversationDetailView {
  return {
    conversation: { id: 1, title: 't', createdAt: 0, updatedAt: 0, archived: false, origin: null },
    messages,
    totalMessages: messages.length,
    lastContextUsage: null,
    turnInFlightSinceSeq: null,
  }
}

describe('buildRailMessages', () => {
  /** One line per tool call, its ok flag from the outcome the backend paired onto it and its path
   *  from its subject, so history renders like a live turn did. A call with no result yet reads ok. */
  it('reads each line off its call block', () => {
    const items = buildRailMessages(
      detail([
        {
          id: 1,
          seq: 0,
          role: 'user',
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
          promptTokens: null,
          completionTokens: null,
          blocks: [{ type: 'text', text: 'what am I looking at?' }],
        },
        {
          id: 2,
          seq: 1,
          role: 'assistant',
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
          promptTokens: null,
          completionTokens: null,
          blocks: [
            { type: 'text', text: 'Checking.' },
            {
              type: 'toolCall',
              callId: 'c1',
              tool: 'list_dir',
              subject: { kind: 'folder', path: '/shots' },
              outcome: { kind: 'refused', reason: { kind: 'problem' } },
              repeatOf: null,
            },
            {
              type: 'toolCall',
              callId: 'c2',
              tool: 'inspect_file',
              subject: { kind: 'files', firstPath: '/shots/a.png', count: 2 },
              outcome: null,
              repeatOf: null,
            },
          ],
        },
        {
          id: 3,
          seq: 2,
          role: 'tool',
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
          promptTokens: null,
          completionTokens: null,
          blocks: [],
        },
      ]),
    )

    expect(items.map((item) => item.kind)).toEqual(['user', 'assistant'])
    const assistant = items[1]
    if (assistant.kind !== 'assistant') throw new Error('expected an assistant item')
    expect(assistant.text).toBe('Checking.')
    expect(assistant.tools).toEqual([
      { callId: 'c1', tool: 'list_dir', running: false, ok: false, path: '/shots' },
      { callId: 'c2', tool: 'inspect_file', running: false, ok: true, path: '/shots/a.png' },
    ])
  })

  /** A model change persists as an `event` row, and renders as its own timeline line. */
  it('renders a persisted model change as a timeline line', () => {
    const items = buildRailMessages(
      detail([
        {
          id: 1,
          seq: 0,
          role: 'event',
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
          promptTokens: null,
          completionTokens: null,
          blocks: [{ type: 'modelChanged', model: 'claude-opus-5' }],
        },
      ]),
    )

    expect(items).toEqual([{ kind: 'modelChange', model: 'claude-opus-5' }])
  })

  /** A chat-memory change persists the same way, so reopening a thread still explains why
   *  the replies after that point saw a different amount of the conversation. */
  it('renders a persisted chat-memory change as a timeline line', () => {
    const items = buildRailMessages(
      detail([
        {
          id: 1,
          seq: 0,
          role: 'event',
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
          promptTokens: null,
          completionTokens: null,
          blocks: [{ type: 'chatMemoryChanged', chatMemoryTokens: 16_000 }],
        },
      ]),
    )

    expect(items).toEqual([{ kind: 'chatMemoryChange', chatMemoryTokens: 16_000 }])
  })

  /** A wake opens its thread with a structured digest sitting in the user-role row. It has
   *  to fold into its own item, not into a text bubble: the digest carries no text at all,
   *  so treating it as one would render an empty bubble where the whole reason for the
   *  thread should be. */
  it('renders a wake digest as its own item rather than an empty user bubble', () => {
    const items = buildRailMessages(
      detail([
        {
          id: 7,
          seq: 0,
          role: 'user',
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
          promptTokens: null,
          completionTokens: null,
          blocks: [
            {
              type: 'wakeDigest',
              folders: [{ folder: '/Users/dana/Downloads', created: 4, modified: 0, removed: 0, renamed: 0 }],
              rollups: [{ ancestor: '/Users/dana/Projects', folders: 7, changes: 40 }],
            },
          ],
        },
      ]),
    )

    expect(items).toEqual([
      {
        kind: 'wakeDigest',
        id: 7,
        folders: [{ folder: '/Users/dana/Downloads', created: 4, modified: 0, removed: 0, renamed: 0 }],
        rollups: [{ ancestor: '/Users/dana/Projects', folders: 7, changes: 40 }],
      },
    ])
  })

  /** What the user answered arrives on two different rows, and both have to fold into the one
   *  item the rail renders: an `event` row the moment they answered, and the user-role opener
   *  of the follow-up turn a rejected sweep earns. The second carries no text either, so
   *  missing it would render an empty bubble where the reason for the turn should be. */
  it('renders a decision the same way whether it arrives as an event or as a follow-up opener', () => {
    const rejected = {
      verb: 'trash' as const,
      what: '/Users/dana/Downloads/*.dmg',
      ops: 12,
      outcome: { kind: 'rejected' as const },
    }
    const items = buildRailMessages(
      detail([
        {
          id: 3,
          seq: 0,
          role: 'event',
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
          promptTokens: null,
          completionTokens: null,
          blocks: [{ type: 'proposalDecisions', decisions: [rejected] }],
        },
        {
          id: 4,
          seq: 1,
          role: 'user',
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
          promptTokens: null,
          completionTokens: null,
          blocks: [{ type: 'proposalDecisions', decisions: [rejected] }],
        },
      ]),
    )

    expect(items).toEqual([
      { kind: 'proposalDecisions', id: 3, decisions: [rejected] },
      { kind: 'proposalDecisions', id: 4, decisions: [rejected] },
    ])
  })
})
