/**
 * Folding a loaded conversation into rail items.
 *
 * A pure transform over the wire types, kept out of the state store beside it: nothing here
 * reads or writes `askCmdrState`, so history rendering is unit-testable on its own.
 */

import type { ConversationDetailView, MessageView } from '#lib/tauri-commands/index.js'
import type { RailMessage, RailToolCall } from './ask-cmdr-messages'
import { outcomeOk, subjectPath } from './ask-cmdr-tool-line'

/** Fold a loaded conversation's messages into rail items, one line per tool call. The backend
 * already paired each call with its result, so a line reads its own outcome off its block. */
export function buildRailMessages(detail: ConversationDetailView): RailMessage[] {
  const out: RailMessage[] = []
  for (const message of detail.messages) {
    if (message.role === 'user') {
      // A wake opens its thread with a structured digest rather than typed prose, so the
      // user-role row can carry either. The digest wins when it's there: a wake never also
      // types something.
      const digest = message.blocks.find((b) => b.type === 'wakeDigest')
      const decisions = message.blocks.find((b) => b.type === 'proposalDecisions')
      if (digest) {
        out.push({ kind: 'wakeDigest', id: message.id, folders: digest.folders, rollups: digest.rollups })
      } else if (decisions) {
        // The opener of the follow-up turn a rejected sweep earns: structured for the same
        // reason a digest is, and it sits where a user bubble would because it is the
        // user-role row of that turn.
        out.push({ kind: 'proposalDecisions', id: message.id, decisions: decisions.decisions })
      } else {
        out.push({ kind: 'user', id: message.id, text: joinText(message), attachments: [] })
      }
    } else if (message.role === 'assistant') {
      out.push({
        kind: 'assistant',
        id: message.id,
        text: joinText(message),
        tools: toolCallsOf(message),
        thinking: false,
        streaming: false,
      })
    } else if (message.role === 'event') {
      for (const block of message.blocks) {
        if (block.type === 'modelChanged') out.push({ kind: 'modelChange', model: block.model })
        if (block.type === 'chatMemoryChanged') {
          out.push({ kind: 'chatMemoryChange', chatMemoryTokens: block.chatMemoryTokens })
        }
        // One decision, as the store recorded it the moment the user answered.
        if (block.type === 'proposalDecisions') {
          out.push({ kind: 'proposalDecisions', id: message.id, decisions: block.decisions })
        }
      }
    }
    // `tool`-role messages carry only results, already folded into the tool lines above.
  }
  return out
}

function joinText(message: MessageView): string {
  return message.blocks
    .filter((b): b is Extract<typeof b, { type: 'text' }> => b.type === 'text')
    .map((b) => b.text)
    .join('')
}

function toolCallsOf(message: MessageView): RailToolCall[] {
  return message.blocks
    .filter((b): b is Extract<typeof b, { type: 'toolCall' }> => b.type === 'toolCall')
    .map((b) => ({
      callId: b.callId,
      tool: b.tool,
      running: false,
      // A call with no result row yet (still running, or a turn that never finished) reads as ok.
      ok: b.outcome ? outcomeOk(b.outcome) : true,
      path: subjectPath(b.subject),
    }))
}
