/**
 * The live turn: sending a message, folding the stream of events into the thread, and
 * stopping.
 *
 * `applyStreamEvent` is the whole reducer — one arm per `AskCmdrStreamEvent`, each mutating
 * the last assistant message in place. Around it sit the two entry points (`sendMessage`,
 * `stopStreaming`) and the progress watchdog that marks a quiet turn stalled and eventually
 * ends it.
 *
 * **The rail subscribes by CONVERSATION, not by send.** Events arrive for every thread the
 * backend is working on, including wakes nobody asked for, so `handleTurnEvent` keeps only
 * the ones about the thread on screen. That is what makes a reload mid-turn recoverable:
 * the rail reloads, lands on the same thread, and the turn it never stopped writing keeps
 * rendering. `adoptLiveTurn` is the other half — after a reload there is no `assistantStarted`
 * left to hear, so any live event is taken as proof a turn is running.
 *
 * **Cancel finalizes locally.** `stopStreaming` finalizes the bubble itself, and the thread goes
 * on the stopped list so a late chunk can't resurrect the "working…" state. The backend's own
 * `stopped` terminal arrives later (the round's calls still run) and only clears that list.
 */

import { getAppLogger } from '#lib/logging/logger.js'
import { refreshRailGate } from './ask-cmdr-gate.svelte'
import { openerItem } from './ask-cmdr-history'
import type { AssistantItem, RailMessage, RailToolCall, Round } from './ask-cmdr-messages'
import type { LineRef, ManagedAiRefusal, ResolvedCall, ToolOutcome, ToolSubject, TurnStats } from '#lib/ipc/bindings.js'
import {
  discardStagedRenameProposals,
  openStagedRenameReview,
  stageRenameProposal,
} from './ask-cmdr-rename-review.svelte'
import { askCmdrState, finalizeAssistant, streamingAssistant } from './ask-cmdr-state.svelte'
import {
  cancelAskCmdr,
  sendAskCmdrMessage,
  type AskCmdrErrorKind,
  type AskCmdrStreamEvent,
  type AskCmdrTurn,
  type MessageView,
  type ProposalDecision,
} from '#lib/tauri-commands/index.js'

const log = getAppLogger('askCmdr')

/** What the turn reducer folds: every event but the one that isn't part of a turn. */
type TurnEvent = Exclude<AskCmdrStreamEvent, { type: 'proposalDecided' }>

/** How long a turn may go without any event before its bubble says so, and before the rail
 * gives up on it entirely. */
const STALL_AFTER_MS = 30_000
const STOP_AFTER_MS = 90_000
let stallTimer: ReturnType<typeof setTimeout> | null = null
let stopTimer: ReturnType<typeof setTimeout> | null = null

/**
 * Threads whose in-flight turn the user stopped. A cancel produces no terminal event, so the
 * backend keeps dribbling for a chunk or two; without this, one of those late events would
 * read as "a turn is live here" and put the rail back into working state with no `done`
 * coming to clear it. Cleared when the thread is sent to again.
 */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping only; nothing renders from it
let stoppedTurns = new Set<number>()

/**
 * The user bubble `sendMessage` put up for the turn now starting, waiting for `userPersisted` to
 * give it its id. Kept by reference because a turn can also open with no bubble typed here, and
 * then the last user bubble on screen belongs to an EARLIER turn.
 */
let typedOpener: Extract<RailMessage, { kind: 'user' }> | null = null

/** Forget every stopped turn and typed opener. Test isolation only; production clears per
 * thread. */
export function resetTurnBookkeeping(): void {
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- see `stoppedTurns`
  stoppedTurns = new Set<number>()
  typedOpener = null
}

/**
 * A thread load put `conversationId` on screen. When the backend says a turn runs there, the rail
 * is in that turn now: the load replays no `assistantStarted`, and the turn may stay quiet for a
 * while (a long tool call), so waiting for its next event to adopt it would leave a running turn
 * looking idle.
 */
export function resumeThread(turnRunning: boolean): void {
  if (turnRunning) adoptLiveTurn()
}

// ── Sending + streaming ──────────────────────────────────────────────────────────

/** Send the user's message and stream the answer. No-ops on empty text or while streaming
 * (single-flight per thread; the composer is disabled mid-turn). */
export function sendMessage(text: string): void {
  const trimmed = text.trim()
  if (!trimmed || askCmdrState.streaming) return
  const attachments = askCmdrState.attachments
  askCmdrState.messages.push({ kind: 'user', id: null, text: trimmed, attachments })
  // Through the state proxy, so `userPersisted` can find it in the list and patch it reactively.
  typedOpener = askCmdrState.messages.at(-1) as Extract<RailMessage, { kind: 'user' }>
  askCmdrState.streaming = true
  resetProgressWatchdog()
  askCmdrState.attachments = []
  if (askCmdrState.conversationId !== null) stoppedTurns.delete(askCmdrState.conversationId)
  // The names the user turned down ride this turn only. Clearing them here (not on the
  // response) keeps a retry from re-sending feedback the model already acted on.
  const deniedNames = askCmdrState.deniedNames
  askCmdrState.deniedNames = []
  void sendAskCmdrMessage(askCmdrState.conversationId, trimmed, attachments, deniedNames).then(
    (outcome) => {
      if (outcome.accepted) {
        askCmdrState.conversationId = outcome.conversationId
        stoppedTurns.delete(outcome.conversationId)
      } else {
        if (askCmdrState.streaming) applyFailed(outcome.kind, outcome.detail, outcome.managed)
        // A switch moved since the rail last looked (in Settings, or in another window).
        // Re-reading turns the rail back into its gate, which carries the way out.
        if (outcome.kind === 'askCmdrOff' || outcome.kind === 'noCloudConsent') void refreshRailGate()
      }
    },
    (e: unknown) => {
      // `sendAskCmdrMessage` turns every Rust refusal into an outcome, so landing here means the
      // invoke itself broke (a missing command, an argument shape that drifted): a Cmdr bug, not
      // the provider. The turn ends as unfinished, with no raw IPC text in front of the person.
      log.error('The send command itself broke, before any provider was involved: {error}', { error: e })
      if (askCmdrState.streaming) applyFailed('unfinishedReply', null)
    },
  )
}

/** Stop the in-flight turn. The runtime sends no terminal event on cancel, so finalize the
 * current bubble locally and stop listening to what the backend is still finishing. */
export function stopStreaming(): void {
  if (!askCmdrState.streaming) return
  if (askCmdrState.conversationId !== null) {
    stoppedTurns.add(askCmdrState.conversationId)
    void cancelAskCmdr(askCmdrState.conversationId)
  }
  finalizeAssistant()
  askCmdrState.streaming = false
  clearProgressWatchdog()
  // A stopped turn may still have staged plans, and a staged plan the user never sees is one
  // they can't answer.
  openStagedRenameReview()
}

/**
 * One turn event off the shared stream. Applies it when it is about the thread on screen and
 * drops it otherwise — a wake thinking in another thread must not write into what the user is
 * reading.
 *
 * A fresh chat is the one case where the rail has no id yet, so a `started` arriving while it
 * waits adopts the id. That can in principle catch a wake's `started` in the same tick; the
 * send's own promise assigns the real id right after and corrects it.
 */
export function handleTurnEvent(turn: AskCmdrTurn): void {
  const { conversationId, event } = turn
  if (event.type === 'proposalDecided') {
    // Not part of a turn, so it takes none of the turn rules below: it adopts no id for a
    // fresh chat, and a stopped thread still shows it.
    noteProposalDecided({ conversationId, messageId: event.messageId, decision: event.decision })
    return
  }
  if (askCmdrState.conversationId === null) {
    if (event.type === 'started' && askCmdrState.streaming) askCmdrState.conversationId = conversationId
    return
  }
  if (conversationId !== askCmdrState.conversationId) return
  if (isTerminal(event)) stoppedTurns.delete(conversationId)
  else if (stoppedTurns.has(conversationId)) return
  applyStreamEvent(event)
}

/** The events that end a turn. They're allowed through on a stopped thread so a `discarded`
 * can still take the thread away. */
function isTerminal(event: TurnEvent): boolean {
  return event.type === 'done' || event.type === 'failed' || event.type === 'stopped' || event.type === 'discarded'
}

/** One answer the user gave to a suggestion, and the thread whose timeline it was written to. */
interface HeardDecision {
  conversationId: number
  /** The decision's timeline row, which is what makes a line heard twice one line. */
  messageId: number
  decision: ProposalDecision
}

/**
 * Decisions heard while a thread load was in flight, for {@link settleDecisionsHeardWhileLoading}
 * to put back. A load replaces the whole message list with what the backend read, and that read
 * can be a moment older than a decision the rail has already heard.
 */
let heardWhileLoading: HeardDecision[] = []

/**
 * The user answered a suggestion: show the line in the thread that made it, and only there.
 *
 * The event carries the decision itself, so the rail adds ONE line instead of re-reading the
 * thread. That is not only cheaper: a re-read replaces the message list, which would throw
 * away an answer that is mid-stream, and its reply could arrive after the user moved to
 * another thread. Nothing here waits on anything, so neither can happen.
 */
function noteProposalDecided(heard: HeardDecision): void {
  if (askCmdrState.loadingHistory) heardWhileLoading.push(heard)
  if (heard.conversationId === askCmdrState.conversationId) showDecision(heard)
}

/**
 * Put back what a thread load may have missed. The trigger calls this when a load ends, however
 * it ended: whatever was heard for the thread now on screen is shown (once), and the rest was
 * about threads the rail isn't showing.
 */
export function settleDecisionsHeardWhileLoading(): void {
  const heard = heardWhileLoading
  heardWhileLoading = []
  for (const decision of heard) {
    if (decision.conversationId === askCmdrState.conversationId) showDecision(decision)
  }
}

/** Add a decision's line, or patch it where it stands when the thread already shows that row.
 * An approval's row is announced twice: "running" at the start, then rewritten with its outcome
 * when it settles. The two come off different threads, and a load can deliver the settled row
 * first, so a late "running" never replaces an outcome. A new line goes above a bubble that is
 * still streaming, which has to stay last for the next chunk to find. */
function showDecision({ messageId, decision }: HeardDecision): void {
  const line: RailMessage = { kind: 'proposalDecisions', id: messageId, decisions: [decision] }
  const at = askCmdrState.messages.findIndex((m) => m.kind === 'proposalDecisions' && m.id === messageId)
  if (at !== -1) {
    if (decision.outcome.kind !== 'running') askCmdrState.messages[at] = line
    return
  }
  insertAboveBubble(line)
}

/** Add `item` just above the streaming bubble, which has to stay last for the next chunk to
 * find, or at the end when no turn is streaming. */
function insertAboveBubble(item: RailMessage): void {
  const assistant = streamingAssistant()
  if (assistant) askCmdrState.messages.splice(askCmdrState.messages.indexOf(assistant), 0, item)
  else askCmdrState.messages.push(item)
}

function applyStreamEvent(event: TurnEvent): void {
  switch (event.type) {
    case 'started':
      return
    case 'queued':
      adoptLiveTurn()
      return
    case 'userPersisted':
      adoptLiveTurn()
      applyUserPersisted(event.messageId, event.opener)
      return
    case 'assistantStarted':
      adoptLiveTurn()
      openRound()
      return
    case 'textDelta':
      adoptLiveTurn()
      applyTextDelta(event.text)
      return
    case 'reasoningTick':
      adoptLiveTurn()
      applyThinking()
      return
    case 'proposalReady':
      // Staged, not shown: a big rename arrives as a run of plans, and the review opens over
      // all of them when the turn ends. See `ask-cmdr-rename-review.svelte.ts`.
      stageRenameProposal(event.proposal)
      return
    case 'done':
      applyDone(event.messageId, event.turn)
      return
    case 'failed':
      applyFailed(event.kind, event.detail, event.managed, event.turn)
      return
    case 'stopped':
      // Terminal: it clears the stopped mark (`handleTurnEvent`). The local stop already
      // finalized the bubble.
      return
    case 'modelChanged':
      insertBeforeCurrentTurn({ kind: 'modelChange', model: event.model })
      return
    case 'chatMemoryChanged':
      insertBeforeCurrentTurn({ kind: 'chatMemoryChange', chatMemoryTokens: event.chatMemoryTokens })
      return
    case 'discarded':
      applyDiscarded()
      return
    default:
      handleLineOrContextEvent(event)
  }
}

/** The events about a round's tool lines, and the two that report on the CONTEXT (what the
 * assembly set aside, and what it cost). Split out so `applyStreamEvent` stays under the
 * complexity ceiling and both halves stay exhaustive. */
function handleLineOrContextEvent(
  event: Extract<
    TurnEvent,
    { type: 'toolCallStarted' | 'toolCallsResolved' | 'toolCallFinished' | 'contextTrimmed' | 'contextUsage' }
  >,
): void {
  switch (event.type) {
    case 'toolCallStarted':
      adoptLiveTurn()
      applyToolStarted(event.callId, event.tool)
      return
    case 'toolCallsResolved':
      adoptLiveTurn()
      applyToolCallsResolved(event.messageId, event.calls)
      return
    case 'toolCallFinished':
      adoptLiveTurn()
      applyToolFinished({ messageId: event.messageId, callId: event.callId }, event.outcome, event.subject)
      return
    case 'contextTrimmed':
      applyContextTrimmed(event.elidedResults)
      return
    case 'contextUsage':
      applyContextUsage(event.estimatedTokens, event.budgetTokens, event.elidedResults)
  }
}

/**
 * Take an event as proof this thread's turn is running.
 *
 * The rail sets `streaming` itself when the user presses Send, so this only bites when it
 * DIDN'T: a reload mid-answer, or opening a thread a wake is still thinking in. Neither
 * replays `assistantStarted`, so any live event has to be the signal.
 */
function adoptLiveTurn(): void {
  if (askCmdrState.streaming) return
  askCmdrState.streaming = true
  resetProgressWatchdog()
}

/** The streaming bubble this turn writes into, created if there is none: the turn's first
 * round, or a turn the rail wasn't watching when it started (a wake, or a reload that landed
 * before the turn's first row). A reload that caught rows of the running turn folds its bubble
 * still streaming, so the rest of the turn lands there. */
function ensureAssistant(): AssistantItem {
  const assistant = streamingAssistant()
  if (assistant) return assistant
  const fresh: AssistantItem = {
    kind: 'assistant',
    id: null,
    // Patched when the opener persists (`userPersisted`), which comes after round 1 streamed.
    turnId: null,
    rounds: [],
    thinking: false,
    stalled: false,
    streaming: true,
    stats: null,
  }
  askCmdrState.messages.push(fresh)
  return fresh
}

/**
 * The round this turn is streaming into, opened if there is none. A round with a `messageId` is
 * SEALED (its row is stored), so a chunk after it starts the next round. That is also what makes
 * a reload mid-turn come out right: every folded round is sealed, so the live chunks that follow
 * open a round of their own instead of writing into a stored one.
 */
function openRound(): Round {
  const assistant = ensureAssistant()
  const last = assistant.rounds.at(-1)
  if (last && last.messageId === null) return last
  assistant.rounds.push({ messageId: null, text: '', calls: [] })
  // The pushed object, through the state proxy, so later writes are reactive.
  return assistant.rounds[assistant.rounds.length - 1]
}

/** Record what this turn's prompt cost, for the footer gauge. One event per answered turn, so
 * this is a straight replace rather than an accumulation. */
function applyContextUsage(estimatedTokens: number, budgetTokens: number, elidedResults: number): void {
  askCmdrState.contextUsage = { estimatedTokens, budgetTokens, elidedResults }
}

/** Show that the budget pushed older lookups out of this turn's context. It goes before the
 * streaming bubble, where the drop happened. */
function applyContextTrimmed(elidedResults: number): void {
  insertAboveBubble({ kind: 'contextTrimmed', count: elidedResults })
}

/**
 * The opener persisted: the user bubble this rail typed for the turn gets its id, and the turn's
 * bubble its turn identity. A turn nobody typed here (a wake, a rejected sweep's follow-up, a
 * send from before a reload) has no such bubble, so its opener goes in above the turn's bubble,
 * unless the thread already shows it.
 */
function applyUserPersisted(messageId: number, opener: MessageView): void {
  const typed = typedOpener
  typedOpener = null
  if (typed && askCmdrState.messages.includes(typed)) typed.id = messageId
  else if (!askCmdrState.messages.some((item) => openerId(item) === messageId)) insertAboveBubble(openerItem(opener))
  const assistant = streamingAssistant()
  if (assistant) assistant.turnId = messageId
}

/** The row id of an item that can open a turn, `null` for every other kind. */
function openerId(item: RailMessage): number | null {
  return item.kind === 'user' || item.kind === 'wakeDigest' || item.kind === 'proposalDecisions' ? item.id : null
}

/** Into the current round even when its calls already started: the wire may interleave a
 * round's text and its `toolCallStarted`s, but the stored row holds the text first, and so does
 * the round. */
function applyTextDelta(text: string): void {
  openRound().text += text
  const assistant = ensureAssistant()
  assistant.thinking = false
  assistant.stalled = false
  resetProgressWatchdog()
}

function applyThinking(): void {
  openRound()
  ensureAssistant().thinking = true
}

/** Idempotent per call within its round: a repeated line crashes the whole window (Svelte's
 * `each_key_duplicate`, ERR-FRMAR). */
function applyToolStarted(callId: string, tool: string): void {
  const round = openRound()
  if (!round.calls.some((candidate) => candidate.callId === callId)) {
    round.calls.push({ callId, tool, running: true, subject: null, outcome: null, repeatOf: null })
  }
  resetProgressWatchdog()
}

/**
 * A round's row persisted, with every call of its final message. The batch is the round's truth:
 * it stamps the row id (sealing the round), gives each line its subject and repeat mark, adds a
 * line whose start never came, and drops a started line the stored message doesn't hold (a
 * nameless call the provider sent and the backend discarded). The k-th call with an id takes the
 * k-th started line with it, as the backend pairs them.
 */
function applyToolCallsResolved(messageId: number, calls: ResolvedCall[]): void {
  const assistant = ensureAssistant()
  let round = assistant.rounds.at(-1)
  if (!round || (round.messageId !== null && round.messageId !== messageId)) round = openRound()
  const started = round.calls
  const taken: RailToolCall[] = []
  round.messageId = messageId
  round.calls = calls.map((resolved) => {
    const line = started.find((candidate) => candidate.callId === resolved.callId && !taken.includes(candidate)) ?? {
      callId: resolved.callId,
      tool: resolved.tool,
      running: true,
      subject: null,
      outcome: null,
      repeatOf: null,
    }
    taken.push(line)
    return { ...line, tool: resolved.tool, subject: resolved.subject, repeatOf: resolved.repeatOf }
  })
  resetProgressWatchdog()
}

/** A call's result row persisted. Found by its line, `(messageId, callId)`: call ids collide
 * across rounds, so the id alone could settle another round's line. */
function applyToolFinished(line: LineRef, outcome: ToolOutcome, subject: ToolSubject): void {
  const call = findLine(line)
  if (call) {
    call.running = false
    call.outcome = outcome
    call.subject = subject
  }
  resetProgressWatchdog()
}

/** The line `ref` names, newest bubble first. Within a round that called one id twice, the first
 * still waiting for its result is the next to finish (results persist in call order). */
function findLine({ messageId, callId }: LineRef): RailToolCall | undefined {
  for (let i = askCmdrState.messages.length - 1; i >= 0; i--) {
    const item = askCmdrState.messages[i]
    if (item.kind !== 'assistant') continue
    const round = item.rounds.find((candidate) => candidate.messageId === messageId)
    if (!round) continue
    const withId = round.calls.filter((candidate) => candidate.callId === callId)
    return withId.find((candidate) => candidate.outcome === null) ?? withId.at(-1)
  }
  return undefined
}

/** The answer round is text-only, so no `toolCallsResolved` stamped it: `done` does, BEFORE the
 * bubble settles, or settling would drop the answer as never stored. */
function applyDone(messageId: number, stats: TurnStats | null): void {
  const assistant = streamingAssistant()
  if (assistant) {
    const last = assistant.rounds.at(-1)
    if (last?.messageId === null) last.messageId = messageId
    else if (last?.messageId !== messageId) assistant.rounds.push({ messageId, text: '', calls: [] })
    assistant.stats = stats
  }
  finalizeAssistant()
  askCmdrState.streaming = false
  clearProgressWatchdog()
  // The turn boundary is what makes a job's batches ONE review.
  openStagedRenameReview()
}

function applyFailed(
  kind: AskCmdrErrorKind,
  detail: string | null,
  managed: ManagedAiRefusal | null = null,
  stats: TurnStats | null = null,
): void {
  const assistant = streamingAssistant()
  if (assistant) assistant.stats = stats
  finalizeAssistant()
  askCmdrState.messages.push({
    kind: 'error',
    errorKind: kind,
    detail: detail ?? undefined,
    managed: managed ?? undefined,
  })
  askCmdrState.streaming = false
  clearProgressWatchdog()
  // Whatever got staged before the failure is real and still the user's to answer.
  openStagedRenameReview()
}

/**
 * The thread the rail is showing was deleted under it: a wake looked, found nothing worth
 * raising, and took its thread with it. There is nothing left to re-read, so the rail steps
 * off it into an empty chat rather than showing a thread that no longer exists.
 */
function applyDiscarded(): void {
  askCmdrState.conversationId = null
  askCmdrState.messages = []
  askCmdrState.messageTotal = 0
  askCmdrState.historyCount = 0
  askCmdrState.contextUsage = null
  askCmdrState.streaming = false
  clearProgressWatchdog()
  // There is no thread left to review against, so this turn's plans go back rather than
  // opening a dialog for a conversation that no longer exists.
  discardStagedRenameProposals()
}

function resetProgressWatchdog(): void {
  clearProgressWatchdog()
  if (!askCmdrState.streaming) return
  stallTimer = setTimeout(() => {
    const assistant = streamingAssistant()
    if (assistant?.streaming) assistant.stalled = true
  }, STALL_AFTER_MS)
  stopTimer = setTimeout(() => {
    if (!askCmdrState.streaming) return
    stopStreaming()
    askCmdrState.messages.push({ kind: 'error', errorKind: 'timeout' })
  }, STOP_AFTER_MS)
}

function clearProgressWatchdog(): void {
  if (stallTimer) clearTimeout(stallTimer)
  if (stopTimer) clearTimeout(stopTimer)
  stallTimer = null
  stopTimer = null
}

/** A timeline line about something that changed between the previous turn and this one (the
 * model, the chat memory size), so it belongs BEFORE this turn's user bubble — which is
 * already rendered optimistically. */
function insertBeforeCurrentTurn(item: RailMessage): void {
  const lastUserIndex = askCmdrState.messages.findLastIndex((m) => m.kind === 'user')
  if (lastUserIndex >= 0) askCmdrState.messages.splice(lastUserIndex, 0, item)
  else askCmdrState.messages.push(item)
}
