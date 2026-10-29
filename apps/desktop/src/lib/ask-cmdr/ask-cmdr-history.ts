/**
 * Folding a loaded conversation into rail items.
 *
 * A pure transform over the wire types, kept out of the state store beside it: nothing here
 * reads or writes `askCmdrState`, so history rendering is unit-testable on its own.
 *
 * **One bubble per turn.** Each assistant row is one round of its turn's bubble, grouped by the
 * `turnId` the backend put on every row; the fold never guesses where a turn starts. What comes
 * out is the bubble the live reducer built for the same turn (`ask-cmdr-turn-parity.test.ts`).
 *
 * **The turn still running stays open.** The backend says since which row a turn has been
 * running (`turnInFlightSinceSeq`). Rows after that mark are its own, so their result-less calls
 * are still running and their bubble still streams, for the live events that follow to write
 * into. Every other result-less call settles with no outcome: its turn ended without one.
 */

import type { ConversationDetailView, MessageView } from '#lib/tauri-commands/index.js'
import type { AssistantItem, RailMessage, RailToolCall, Round } from './ask-cmdr-messages'
import { settleRounds } from './ask-cmdr-rounds'

/** Fold a loaded page of a conversation into rail items. */
export function buildRailMessages(detail: ConversationDetailView): RailMessage[] {
  const since = detail.turnInFlightSinceSeq
  const out: RailMessage[] = []
  // The bubble the current turn's rounds go into, once its first assistant row has come by.
  let open: AssistantItem | null = null
  for (const message of detail.messages) {
    const running = since !== null && message.seq > since
    if (message.role === 'user') {
      open = null
      out.push(openerItem(message))
    } else if (message.role === 'assistant') {
      if (open?.turnId !== message.turnId) {
        open = emptyBubble(message.turnId)
        out.push(open)
      }
      addRound(open, message, running)
    } else if (message.role === 'event') {
      placeEvent(out, open, message, running)
    }
    // `tool`-role rows carry no blocks: each result rides its call's block.
  }
  for (const bubble of out) {
    if (bubble.kind === 'assistant' && !bubble.streaming) bubble.rounds = settleRounds(bubble.rounds)
  }
  return out.filter((item) => item.kind !== 'assistant' || item.rounds.length > 0)
}

/** An event row's lines. One recorded while its turn still ran goes above that turn's bubble,
 * where the live rail put it (a bubble that is still streaming stays last). Past the running
 * turn's newest row the turn is still going, so a line there is during it too. */
function placeEvent(out: RailMessage[], open: AssistantItem | null, message: MessageView, running: boolean): void {
  const hoist = open !== null && (message.duringTurn || running) && open.turnId === message.turnId
  out.splice(hoist ? out.indexOf(open) : out.length, 0, ...eventItems(message))
}

/** One assistant row onto its turn's bubble: a round, the bubble's id, and the turn's stats when
 * the row carries them. A row of the running turn keeps its bubble streaming. */
function addRound(bubble: AssistantItem, message: MessageView, running: boolean): void {
  bubble.rounds.push(roundOf(message, running))
  bubble.id = message.id
  if (running) bubble.streaming = true
  if (message.turnStats) bubble.stats = message.turnStats
}

/** The row that opens a turn. A wake opens its thread with a structured digest rather than typed
 * prose, and a rejected sweep opens its follow-up turn with the decisions, so the user-role row
 * can carry either. Those win when they're there: neither also types something. */
export function openerItem(message: MessageView): RailMessage {
  const digest = message.blocks.find((b) => b.type === 'wakeDigest')
  if (digest) return { kind: 'wakeDigest', id: message.id, folders: digest.folders, rollups: digest.rollups }
  const decisions = message.blocks.find((b) => b.type === 'proposalDecisions')
  if (decisions) return { kind: 'proposalDecisions', id: message.id, decisions: decisions.decisions }
  return { kind: 'user', id: message.id, text: joinText(message), attachments: [] }
}

function emptyBubble(turnId: number | null): AssistantItem {
  return {
    kind: 'assistant',
    id: null,
    turnId,
    rounds: [],
    thinking: false,
    stalled: false,
    streaming: false,
    stats: null,
  }
}

/** One assistant row as its round. A call with no result row yet is still running when its row
 * belongs to the running turn, and settled with no outcome otherwise. */
function roundOf(message: MessageView, running: boolean): Round {
  const calls: RailToolCall[] = message.blocks
    .filter((b): b is Extract<typeof b, { type: 'toolCall' }> => b.type === 'toolCall')
    .map((b) => ({
      callId: b.callId,
      tool: b.tool,
      running: running && b.outcome === null,
      subject: b.subject,
      outcome: b.outcome,
      repeatOf: b.repeatOf,
    }))
  return { messageId: message.id, text: joinText(message), calls }
}

/** An `event`-role row's timeline lines. A row with no blocks (a turn's stats record, or a kind
 * this build doesn't know) draws nothing. */
function eventItems(message: MessageView): RailMessage[] {
  const items: RailMessage[] = []
  for (const block of message.blocks) {
    if (block.type === 'modelChanged') items.push({ kind: 'modelChange', model: block.model })
    if (block.type === 'chatMemoryChanged') {
      items.push({ kind: 'chatMemoryChange', chatMemoryTokens: block.chatMemoryTokens })
    }
    // One decision, as the store recorded it the moment the user answered.
    if (block.type === 'proposalDecisions') {
      items.push({ kind: 'proposalDecisions', id: message.id, decisions: block.decisions })
    }
  }
  return items
}

function joinText(message: MessageView): string {
  return message.blocks
    .filter((b): b is Extract<typeof b, { type: 'text' }> => b.type === 'text')
    .map((b) => b.text)
    .join('')
}
