# Ask Cmdr tool transparency plan

Issue: [#748](https://github.com/vdavid/cmdr/issues/748) (tool lines say only THAT a tool ran, never what it looked at
or what came back). Plan written 2026-10-26 against `70ae9eb83c`, revised after review rounds 1–4. All eight scope
points in the issue ship; "point N" below means item N of its "Scope" list.

The idea in one line: the backend turns every tool call's arguments and result into TYPED facts (`ToolSubject`,
`ToolOutcome`, `ToolPreview`, a repeat mark, turn stats), computed by pure functions from what `main.db` already stores,
so the live stream and a reloaded thread render the same thing and no migration is needed. The frontend folds a turn
into ROUNDS (one per model call: its narration, then its tool calls), merges adjacent tool groups for display, and owns
every word.

## Decisions (David's, fixed)

1. All eight scope points ship.
2. **Expanded view** = a curated preview of what the model received (first N rows, matches, a truncation note like
   "showed the model 200 of 4,812"), plus a small, low-key "Copy" button that copies the raw JSON tool result: the real
   thing the model saw, for advanced users.
3. **Group collapse** is a setting in Settings > Advanced with two values: collapse when the answer arrives, or stay as
   the user left it. David wants to try both.
4. **Paths in lines are clickable** and navigate a pane there, through the app's existing reveal primitives.

## Decisions made in this plan (the lead's; David may overturn)

1. **No schema migration.** Subjects, outcomes, previews, and repeat marks are DERIVED on read from the stored
   `AgentToolCall.arguments` and `AgentToolResult.content`, by the same pure functions the live path uses. Old threads
   get the new view for free. Elision is assembly-only (`chat/context.rs`): the stored content is the full result the
   model read in its own turn. A stored row that IS marked `elided: true` gets a "set aside" fallback (decision 13).
2. **Turn stats persist as a new `ConversationEvent::TurnFinished` event row**, appended after the turn's last row when
   the turn ran at least one tool. Wall time can't be derived on reload (every row of a turn carries the same
   `params.now_secs`), so it must be stored; an event row needs no migration. An old turn with no such row gets fallback
   stats computed in the BACKEND projection (call count, summed row tokens, `wallMs: null`).
3. **Every turn that ran tools gets stats and a footer**, answered, failed, or cancelled: what the agent spent matters
   most when it didn't deliver. Stats ride the TERMINAL event (`done`, `failed`, and a new `stopped`), never a trailing
   one: any non-terminal event after a turn ends would re-adopt it as live (`adoptLiveTurn`).
4. **The raw result crosses IPC only on demand**: `ask_cmdr_tool_result(conversationId, messageId, callId)` returns the
   preview and the pretty-printed raw JSON when the user expands a line. Never in the stream, never in the history page.
   It reaches only the user's own screen and clipboard, but it changes a stated invariant ("no raw tool-result content
   ever crosses": `commands/agent/views.rs` header, `store/CLAUDE.md`), so those docs change in the same commit.
5. **A line is identified by `(messageId, callId)`, never `callId` alone.** Call ids are NOT unique per thread or even
   per turn: the fake mints `fake-call-{index}` per `respond` (`llm/fake.rs:113`), and Gemini via genai resets them per
   response. `messageId` is the id of the assistant row (one per `respond` round) that made the call.
6. **One assistant bubble per turn, made of rounds.** A round is one `respond` call: `{ messageId, text, calls }`.
   Stored assistant rows are NOT in stream order: genai's `StreamEnd::from` (`chat_stream.rs:131`) yields the thought
   signature, ONE concatenated text, then all tool calls, and `build_agent_message_from_stream_end` keeps that. So the
   canonical shape of a round is `[text?, tools?]`, and the live reducer routes a round's text deltas into that round's
   text, placed before its calls, even when deltas and `toolCallStarted` interleave on the wire. Display merges two
   adjacent tool groups when the round between them has no text (point 4's "consecutive calls").
7. **Repeat key = the repeat breaker's key** (tool wire name + canonical args JSON). It moves from
   `chat/runtime/repeats.rs` onto `AgentToolCall` so the breaker, the live mark, and the reload projection share ONE
   definition. Scope: one turn (a turn = one `drive` = rows between two user-role rows).
8. **On reload, groups render collapsed under both setting values**: "as the user left it" has nothing to remember
   across a reload. Live, the setting decides what happens when the answer arrives. Default: collapse on answer.
9. **A `problem` sentence shows verbatim in the preview** (plain text, labeled as what the model was told), the same
   precedent as `Failed.detail`. The outcome line itself stays typed and localized.
10. **Memory refusals become not-ok.** `dispatch_ok` treats `{saved: false, refused, detail}` as ok (no `problem` key);
    the new outcome makes it `Refused(Memory)` and the glyph turns to the refusal glyph. Intended: the user should see
    that a note wasn't saved. `dispatch_ok` itself (repeat breaker, analytics) doesn't change.
11. **Downgrades, accepted knowingly**: an app version from before the tolerant parse can't load, or even SEND into, a
    thread holding a `TurnFinished` row (`list_messages` feeds both the history page and `load_transcript`). Open beta,
    auto-update, rare downgrades. The tolerant parse is the branch's first commit (a separate green commit, no separate
    land: only the lead lands, once, at the end), and `store/DETAILS.md` states the impact plainly.
12. **The soft-cap nudge keeps today's semantics** by counting user items + ROUNDS + event items. Today it counts rail
    items, where every assistant row was its own bubble; folding a turn into one bubble would otherwise make it fire far
    later. No wire field is needed.
13. **A set-aside (`elided: true`) stored result** derives `ToolOutcome::SetAside` and a preview that says so; a stub
    never shows as what the model saw. If milestone 1 finds that nothing ever persisted `elided: true`, `SetAside` is
    dropped and such a row maps to `Answered` plus a preview `setAside` flag (defensive only).
14. **Running state on reload comes from the backend**: `chat/cancel.rs` registers each in-flight turn together with a
    seq high-water mark (the thread's last seq when the turn started), and the conversation detail carries
    `turnInFlightSinceSeq: Option<i64>`. Only result-less calls in rows AFTER the mark fold as `running: true`; every
    other result-less call is settled with no outcome ("no answer recorded"), so a crashed turn never spins forever. If
    no row exists after the mark yet (reload before the first `End`), the fold leaves the last bubble closed and the
    first live event opens a NEW bubble; `UserPersisted` carries the opener's view so the rail can insert the user item
    it never loaded. `loadConversation` calls the adopt path (`adoptLiveTurn`) when the mark is present.
15. **Stop is local first, settled by the backend.** Dispatch isn't cancellable (`turn.rs:270-315` never checks cancel),
    so after a Stop the round's remaining calls still run and persist. The first terminal event after a local stop
    (`done`, `failed`, or `stopped`) belongs to the stopped turn: a per-conversation "pending stop → bubble ref" that
    ANY terminal consumes settles that bubble and attaches stats, and never touches `streaming`, the watchdog, or the
    rename review. Between the click and that terminal, events still apply to the stopped thread WITHOUT
    `adoptLiveTurn`: `toolCallsResolved` to the pending-stop bubble's last unresolved round, `toolCallFinished` and
    `proposalReady` by `LineRef`. Lines still running settle at the terminal event, not at the click. A `stopped` with
    NO pending local stop (the status corner stopping a wake, `wake/runner.rs:190-195`) finalizes like `done`: finalize,
    `streaming = false`, stats, `openStagedRenameReview()`. That also fixes today's bogus 90 s watchdog "timeout" after
    a wake stop.
16. **Turns are identified by the backend.** Every `MessageView` carries `turnId: Option<i64>` (the opener row's id: the
    user-role row starting its turn; `None` for rows before the thread's first user row) and every event view
    `duringTurn: bool` (it sits before its turn's last assistant row). The fold groups by `turnId`, the page seam merges
    bubbles with the same `turnId`, and the hoist reads `duringTurn`. No frontend heuristic guesses turn edges.

## Loud rules (every milestone)

- ❗ **Read first**: `apps/desktop/src-tauri/src/agent/CLAUDE.md` + `DETAILS.md`, `agent/chat/CLAUDE.md` + `DETAILS.md`,
  `agent/tools/CLAUDE.md` + `DETAILS.md`, `agent/store/CLAUDE.md`, `apps/desktop/src/lib/ask-cmdr/CLAUDE.md` +
  `DETAILS.md`, and this whole plan. If a line ref here is off, trust the code and keep the intent.
- ❗ **Typed across IPC, never prose.** Every new wire field is a specta-typed enum, number, path, or the model's own
  text. ❌ No English sentence from Rust becomes UI copy, except the verbatim `problem` inside the expanded preview
  (decision 9). ❌ No classification by string-matching (`error-string-match`, `cmdr/no-error-string-match`): outcomes
  read OUR OWN result KEYS (`available`, `repeatedCall`, `problem`, `refused`, `readyForReview`, `total`, ...) or typed
  tokens, the way `tool_result_ok` and `reports_a_problem` already do, never a message's wording.
- ❗ **A line's identity is `(messageId, callId)`** everywhere: events, reducer lookups, the projection's annotation
  map, the repeat mark, the result command, Svelte `each` keys. ❌ Never look a line up by `callId` alone.
- ❗ **Smart backend, thin frontend.** Subject, outcome, preview, repeat mark, and turn stats (including old-turn
  fallbacks) are computed in Rust. The frontend picks localized templates by variant, folds rounds, merges groups, and
  formats numbers. ❌ No `JSON.parse` of tool arguments in the frontend (`pathFromArguments` goes away).
- ❗ **Model text and filenames are untrusted.** Narration renders only through `renderAssistantMarkdown`. Paths, names,
  queries, snippets, and the raw JSON render as plain `{text}`, ❌ never `{@html}`.
- ❗ **The raw result is private data.** It can hold `inspect_file` text windows and EXIF incl. GPS. ❌ Never log it
  (backend or frontend logger, so never into an error report), and cache it outside deep `$state` (`$state.raw` or a
  plain `Map`), so devtools-style reactive snapshots and proxies never hold it.
- ❗ **Nothing the model sees changes**, except the optional system-prompt nudge in the last milestone. ❌ Don't add
  keys to tool results to make outcomes easier to classify: that costs every user tokens to buy a UI convenience. The
  prefix stays byte-identical across a thread's calls (`chat/CLAUDE.md`).
- ❗ **Backward compat**: a thread written before this work (no `TurnFinished`, arguments of any shape, results of any
  shape, an `Unrecognized` tool, colliding call ids) must load and render. Every derivation is total: an unexpected
  shape yields a fallback (`ToolSubject::Unparsed`, `ToolOutcome::Answered` / `Refused(Problem)`), ❌ never a panic or a
  failed load.
- ❗ **i18n**: English strings only, each with an `@key` description, in the `askCmdr.*` catalog under
  `src/lib/intl/messages/en/` (check the file). ❌ Never translate. Counts go in as `{countText}` + `{count}` (the
  catalogs' `*Text` convention), sizes through `#lib/units`, durations through `formatMilliseconds` (or convert to
  `Seconds` for `formatDuration`; it takes seconds), lists through `#lib/intl/list-format`. Run `pnpm intl:keys` (in
  `apps/desktop`) after adding keys, and `node apps/desktop/scripts/sync-locale-keys.ts` after adding a settings
  `keywordsKey`.
- ❗ **Never add a placeholder to an existing key's value**: the 11 translated catalogs still carry the old value, and
  `desktop-i18n-parity` fails in every locale. A new placeholder set gets a NEW key. A retired key is deleted from `en`
  AND every locale catalog in the same commit (deleting isn't translating), per `src/lib/intl/messages/DETAILS.md`'s
  removal steps.
- ❌ **Never use string tool names as a type.** The wire `tool` stays a string for the label map, but subject and
  outcome are discriminated unions; frontend maps over them are `Record<Kind, ...>` so a new variant is a compile error.
- **TDD**: each derivation and reducer change gets a red test first, failing for the right reason.
- **Docs**: every milestone updates the C+D.md of the dirs it touches (decisions in `D.md`, one-line guardrails in
  `C.md` only where ignoring them breaks something silently). New module dir ⇒ new C+D pair.
- **Gates per commit** (`AGENTS.md` § Checker script): `pnpm check --fast && pnpm check svelte-tests-related`, plus
  `pnpm check clippy` and the touched Rust tests (`cargo nextest run <name>`) when Rust changed. Regenerate bindings
  with `pnpm bindings:regen` (in `apps/desktop`) after any specta change. Foreground commands only.
- **Reach for the house primitives** (`docs/guides/building-ui.md`): path links are `LinkButton`; a path inside a
  sentence goes through `<Trans>` (`src/lib/intl/Trans.svelte`) with a `LinkButton` snippet; Copy is
  `Button variant="secondary" size="mini"` calling `copyToClipboard` from `#lib/tauri-commands` (as `CopyBox.svelte`
  does).
- **Strip milestone tags** from code, comments, test names, and docs before reporting
  (`docs/guides/multi-agent-refactors.md`).
- Each executing agent reports in ≤350 words: what landed, what deviated from this plan and why, what's left.

## Census (fresh, `70ae9eb83c`)

Backend, the turn loop (`apps/desktop/src-tauri/src/agent/chat/runtime/turn.rs`, `drive`):

- `consume_stream` forwards `AgentDelta::ToolCallStarted { call_id, tool }` as `AgentChatEvent::ToolCallStarted` WHILE
  the call's arguments are still streaming, so arguments are unknown then. genai emits it once per call id
  (`llm/genai_impl.rs`), the fake once per call. A nameless degenerate call gets a `ToolCallStarted` and is then DROPPED
  from the final message (`genai_impl`, `llm/DETAILS.md` § Degenerate tool calls).
- After `End`, the `AgentMessage` holds parts in genai's `StreamEnd` order (decision 6), not stream order. It is
  persisted as one assistant row with that call's `prompt_tokens` / `completion_tokens`.
- The dispatch loop: `FailedCalls::judge` (repeat breaker) → `dispatcher.dispatch` → **emits
  `ToolCallFinished { call_id, ok }` BEFORE `store::append_message` persists the tool row**, so a lazy fetch could race
  it.
- Terminal events are emitted from inside `drive`'s many return paths: `Done { message_id, seq, stop, usage }` (`usage`
  = the LAST `respond` call's only) and `Failed { kind, detail }`; a cancel returns `TurnResult::Cancelled` and emits
  NOTHING. `TurnTally::tool_turns` counts rounds, not calls. `started: Instant` exists in `drive`.
- `repeats.rs::key(call)` = `"{wire_name}\u{1f}{arguments}"` (canonical: `serde_json::Map` is a BTreeMap). `annotate`
  KEEPS the original `problem` and adds `repeatedCall` + `guidance` (`repeats.rs:97-106`).
- Elision (`chat/context.rs`) replaces content in the ASSEMBLED prompt only; `MIN_ELISION_TURNS_BACK = 1`.

Backend, transport and projection:

- `agent/chat/stream.rs`: `AskCmdrStreamEvent` is the wire projection (specta, `type`-tagged camelCase).
- `commands/agent/views.rs::to_message_view` projects parts to `MessageBlock`:
  `ToolCall { call_id, tool, arguments: String }` and `ToolResult { call_id, ok, elided }`. `ask_cmdr_get_conversation`
  pages via `store::get_conversation`, projecting each row on its own. `commands/agent/chat.rs::slot_event_view`
  (~line 126) also projects event rows.
- `agent/store/events.rs::ConversationEvent` (serde `tag = "kind"`): `ModelChanged`, `ChatMemoryChanged`,
  `ProposalDecided`. `query.rs` parses event rows with `?`, so an unknown variant fails the whole page load AND
  `load_transcript` (both go through `list_messages`).
- Tools (`ToolId`, `llm/types.rs`): `AppState`, `ListDir`, `ListPaneFiles` (NO arguments: folder and scope live in its
  result, `read/pane_listing.rs:80-142`), `ImportantFolders`, `FolderImportance`, `ListVolumes`, `InspectFile`,
  `OperationsList`, `OperationsGet`, `Search` (`scope` is a comma list with `!` excludes), `SearchPhotos`, `ImageFacts`,
  `ProposeRenamePlan`, `ListSuggestions`, `GetSuggestionGroup`, `ProposeSuggestions`, `NothingToSuggest`, `MemoryWrite`
  / `MemoryEdit` (take `path`), plus `Unrecognized(String)`. Result vocabulary: `agent/tools/DETAILS.md` § The tool
  catalog and § The size contract, `mcp/executor/DETAILS.md` § The search result, `agent/tools/suggestions/`,
  `agent/tools/propose/`, `agent/tools/memory.rs`. Refusal shapes: `{available: false, requested, reason}` (view gate),
  `{problem}` (schema gate, `ToolError`, handler problem), `{problem, repeatedCall, guidance}` (breaker),
  `{saved: false, refused: <MemoryRefusal token>, detail}` (memory), `readyForReview: false` stamped on every propose
  problem (`view.rs::ensure_review_verdict`).
- `chat/context.rs::THREAD_SOFT_CAP_MESSAGES = 40` (rows); `chat/cancel.rs` registers in-flight turns by conversation.

Frontend (`apps/desktop/src/lib/ask-cmdr/`):

- `ask-cmdr-messages.ts`: `RailToolCall { callId, tool, running, ok, path }`; assistant item
  `{ text, tools[], thinking, stalled, streaming }`.
- `ask-cmdr-stream.svelte.ts`: `applyTextDelta` appends to one `text`; `applyToolStarted` pushes **`path: null` always**
  (the dead UI the issue names); `applyToolFinished` finds the line by `callId` alone across the thread; `adoptLiveTurn`
  treats any non-terminal event as a live turn; `isTerminal` = `done` / `failed` / `discarded`; `stopStreaming`
  finalizes locally and drops later events for the thread (`stoppedTurns`).
- `ask-cmdr-history.ts::buildRailMessages`: one item per assistant ROW, `ok` from result blocks by `callId`, `path` via
  `pathFromArguments`.
- `AskCmdrToolLine.svelte`: one `<button>` row (glyph + label + path + chevron), expands to the path only.
- `ask-cmdr-state.svelte.ts::isOverSoftCap` counts rail items (line 122).
- `ask-cmdr-trigger.svelte.ts`: "load earlier" PREPENDS `buildRailMessages(older)` with no seam handling; a 12-call turn
  is ~20 rows against a 50-row page, so seams will split turns routinely.
- Labels: `ask-cmdr-labels.ts` `TOOL_LABEL_KEYS` (doing/done per tool, `UNKNOWN_TOOL_KEYS` fallback).
- Navigation: `#lib/file-explorer/navigation/navigate-and-select.ts` (`resolveLocationOrToast`,
  `navigateToDirInBestPane`, `revealFileInBestPane`, typed against `PaneRevealAPI`). The rail gets explorer access only
  by prop from `routes/(main)/+page.svelte` (`<AskCmdrRail readSelection=... />`).
- Settings: `src/lib/settings/definitions/advanced.ts` (auto-rendered by `AdvancedSection.svelte`, grouped by `cardKey`;
  no Ask Cmdr card yet), `types.ts` `SettingsValues`. Guide: `docs/guides/adding-a-new-setting.md`.
- E2E: `test/e2e-playwright/ask-cmdr.spec.ts` drives a `FakeAgentLlm` under `CMDR_E2E_ASK_CMDR_FAKE=1` with a Say-only
  script. `i18n-capture-ask-cmdr.ts` photographs rail surfaces for translators.

## The wire vocabulary (decided here; milestone 1 builds it)

Names are intent; the executing agent may refine them if the code argues for it, and must say so in its report.
Everything is `serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")` + `specta::Type`.

`LineRef { messageId: i64, callId: String }`: a line's identity (decision 5).

`ToolSubject`, what the call looked at, from `subject_of(call, result: Option<&Value>)`. The result refines it where the
arguments can't say (`list_pane_files`); live, `toolCallsResolved` carries the argument-only subject and
`toolCallFinished` the refined one; reload uses the same function with the result:

- `Panes` (`app_state`); `FocusedPane { folder: Option<path>, selectionOnly: Option<bool> }` (`list_pane_files`, from
  its RESULT; both `None` before it returns)
- `Folder { path }` (`list_dir`, `folder_importance`)
- `Files { firstPath, count }` (`inspect_file`, `image_facts`; count = paths in the call)
- `Search { query: Option<String>, scopePaths: Vec<String> }` (`search`: pattern plus the scope's INCLUDE paths only,
  split from its comma list, `!` excludes dropped) and `PhotoSearch { query }` (`search_photos`). ❌ Never hand a raw
  scope string to `revealPath`.
- `ImportantFolders`, `Drives` (`list_volumes`)
- `OperationLog` (`operations_list`), `Operation { id }` (`operations_get`)
- `RenamePlan { count }`, `Suggestions` (`list_suggestions`), `SuggestionGroup` (`get_suggestion_group`),
  `SuggestionSweep { count }` (`propose_suggestions`), `NothingToSuggest`
- `MemoryFile { path, absolute: Option<path> }` (`memory_write`, `memory_edit`; the arg is `path`, RELATIVE to the
  memory folder, `memory.rs:32`). Rust resolves it against the memory root through `memory/`'s own jail; `absolute` is
  `Some` only when it resolves inside the folder, and only then is the path clickable (else plain text).
- `UnknownTool { name }` (`Unrecognized`; a stored legacy nameless call, `Unrecognized("")`, maps to the generic unknown
  label), `Unparsed` (a known tool whose arguments don't carry what we expect)

Every path in a subject is `~`-expanded in Rust, so the frontend gets absolute paths. Verify every argument name against
the tool's schema (`agent/tools/read/*.rs`, `mcp/tool_registry/`, `mcp/executor/*.rs`).

`ToolOutcome`, what came back, from `outcome_of(tool, result: &AgentToolResult)`. Check order matters: `elided` first,
then `available: false`, then `repeatedCall` (the breaker keeps `problem` in its annotation, so it must win over
`problem`), then for propose tools `readyForReview` (`ensure_review_verdict`, `view.rs:61-88`, stamps
`readyForReview: false` onto every propose problem, so a propose problem is `NotStaged`, with the `problem` sentence
verbatim in its preview), then `problem`, then `refused` (memory), then the per-tool success shapes. On the `elided`
check: before writing it, `git log -S 'elided: true'` (and read the store writers) to see whether any code path ever
PERSISTED `elided: true`. If none did, drop `SetAside` and map such a row to `Answered` plus a preview `setAside` flag
(defensive), and say so in the report.

- `Items { returned, total: Option<u64>, lowerBound: bool }`: a paged list (`total` / `returned` / `truncated`)
- `Matches { returned, matchCount: u64, lowerBound: bool }`: `search` (⚠️ `matchCount` counts past the cap, never a
  `total`; `stillWalking` / `abandonedGround` make it a lower bound) and `search_photos`
- `NothingFound { lowerBound: bool }` (a lower bound says "none found yet", ❌ never "no matches")
- `Answered`: a single-object answer (`app_state`, `folder_importance`, `operations_get`, `nothing_to_suggest`)
- `Staged { count }` / `NotStaged`: propose tools, by `readyForReview`
- `Saved` (memory, `saved: true`)
- `SetAside` (the stored row is `elided: true`; decision 13)
- `Refused { reason: RefusalReason }`: `ToolUnavailable` (`available: false`), `RepeatedCall` (breaker), `Problem`
  (schema gate or handler problem; the JSON can't tell them apart and we won't add a key),
  `Memory { kind: MemoryRefusalKind }` (decision 10)
- `ok(&self) -> bool`: `Refused` and `NotStaged` are not ok; everything else is. It agrees with `dispatch_ok` on every
  fixture EXCEPT memory refusals (decision 10), and a test says so explicitly (a propose problem is `NotStaged` and not
  ok, which agrees with `dispatch_ok`).

`MemoryRefusalKind`: a fieldless enum in `agent/memory/` mirroring `MemoryRefusal`'s variants plus `Unavailable`
(unconditionally: the memory folder couldn't be reached) and `Other`, with `MemoryRefusalKind::from_token(&str)`
(unknown token → `Other`) and `MemoryRefusal::kind()`, which never returns `Unavailable` or `Other`.
`MemoryRefusal::token()` is re-expressed through the kind's token so the two can't drift; a test round-trips every
variant.

`ToolPreview`, the expanded view (only via `ask_cmdr_tool_result`):

- `rows: Vec<PreviewRow>` (first `PREVIEW_ROWS`, start at 20), each
  `{ name, path: Option, kind: File|Folder|Other, sizeBytes: Option<u64>, line: Option<u32>, snippet: Option<String> }`
- `rowsGivenToModel: Option<u32>`, `total: Option<u64>`, `lowerBound: bool` for the truncation note
- `problem: Option<String>` (verbatim, decision 9), `setAside: bool` (decision 13: no rows then)
- Tools with no list (`app_state`, `folder_importance`, memory, ...) return NO rows, and the frontend shows a short note
  pointing at Copy for the full result. ❌ No per-tool `facts` variant. The preview never claims the model saw rows it
  didn't.

`ToolResultDetailView { preview: ToolPreview, rawJson: String }`, returned by `ask_cmdr_tool_result`, whose `Err` is its
own small typed enum (`NotFound`, `StoreUnavailable`), per `commands/CLAUDE.md`.

`TurnStatsView { toolCalls: u32, wallMs: Option<u64>, promptTokens: u32, completionTokens: u32 }` (sums across the
turn's rounds; `wallMs` is `None` only in the old-turn fallback). Tokens are the same per-row `prompt_tokens` /
`completion_tokens` the cost meter sums, so per-turn footers add up to the thread's `AskCmdrCostFooter` figures.
Terminal events carry it as `turn: Option<TurnStatsView>`: `None` when no tool ran or no turn started (the three
`Failed` emits in `commands/agent/chat.rs` ~238/301/334 refuse before any turn), so a tool-less turn looks the same live
and reloaded.

## Milestones

Nine, sequential (1, 2a, 2b, 3a, 3b, 4–7), one agent each. Each leaves the branch green. Only the lead lands, once, at
the end.

### 1. Tolerant event rows, then the backend vocabulary (M)

**Scope, commit 1 (its own commit, first)**: `ConversationEvent` gains `#[serde(other)] Unknown`, a unit variant that is
NEVER written (assert that in `append_event`, debug-assert or typed so it's unrepresentable at the call sites). An
unknown `kind` now parses to `Unknown` and the ROW IS KEPT (paging counts rows, so dropping it would break offsets);
`to_message_view` (`views.rs:320`) and `slot_event_view` (`commands/agent/chat.rs:126`) project it to an `Event` view
with no blocks; the transcript loader already ignores event rows. `store/DETAILS.md` states decision 11 plainly. Tests:
a row with an unknown kind loads as an empty event view at its seq, the thread still sends, and paging totals hold.

**Scope, the rest**: a new module `apps/desktop/src-tauri/src/agent/tools/line/` (named after the UI's "tool line"; C+D
pair, added to `agent/tools/CLAUDE.md`'s module map) holding the vocabulary above and the pure functions:

- `subject_of(call: &AgentToolCall, result: Option<&Value>) -> ToolSubject`
- `outcome_of(tool: &ToolId, result: &AgentToolResult) -> ToolOutcome` (check order above)
- `preview_of(tool: &ToolId, result: &AgentToolResult) -> ToolPreview`
- `AgentToolCall::identity_key()` moved from `repeats.rs::key` (the breaker calls the method; its tests stay green)
- `repeat_marks(calls: &[(i64 /*messageId*/, &AgentToolCall)]) -> HashMap<LineRef, LineRef>` for one turn (each repeat →
  the FIRST identical call of the turn)
- `MemoryRefusalKind` in `agent/memory/` (above)

**Intentions**: one place answers "what did this call look at, and what came back" for every `ToolId`, from data the
store already holds. Exhaustive `match` on `ToolId` (no `_` arm), and add "a `line/` subject + outcome arm" to the "New
agent tool =" checklist in `agent/tools/CLAUDE.md`.

**Landmines**:

- Result shapes are serde output of typed structs; read their FIELD names, and pin each extraction with a fixture built
  from the real struct where constructible in a unit test (else a JSON fixture copied from a handler test).
- `search`: read `mcp/executor/DETAILS.md` § The search result first. `countOnly: true` returns no rows but real counts.
  The scope parse must survive commas inside quoted paths if the schema allows them; mirror the executor's own split
  (reuse it if it's callable), never a second grammar.
- `inspect_file` rows carry per-path statuses (`missing`, `unreadable`, `unreachable`, ...); a `find` hit's `line` +
  `text` become the preview row's `line` + `snippet`.
- `list_pane_files`: the subject comes from its result; with no result (still running, refused) it's `FocusedPane` with
  both fields `None`.
- `~` expansion: use the crate's existing helper (the `expand_tilde` family), not a hand-rolled one; only for local
  paths.
- Bound everything: paths, queries, names, and snippets are untrusted and can be huge; cap strings (say 512 chars).

**Test plan**: a table test per tool (args → subject, args + result → refined subject, result → outcome, result →
preview with row cap and truncation fields). Plus: malformed args → `Unparsed`; unknown result shape → fallback, no
panic; `Unrecognized` → `UnknownTool` + `Refused(ToolUnavailable)`; a breaker-annotated result → `RepeatedCall`, not
`Problem`; an elided row → `SetAside`; memory refusal → `Refused(Memory)` with the right kind;
`outcome.ok() == dispatch_ok` over all fixtures except memory refusals (asserted the other way); repeat marks across two
rounds that REUSE call id `fake-call-0` (identity by `LineRef`), with argument keys in different order.

**DONE**: commit 1 is a separate green commit; module + tests green, clippy green, nothing else wired, docs updated.

### 2a. Live wiring: events, terminal stats in `run_turn`, the `TurnFinished` row (M)

**Scope**:

- **Events** (`AgentChatEvent` + `AskCmdrStreamEvent`):
  - New `ToolCallsResolved { message_id, calls: Vec<{ call_id, subject, repeat_of: Option<LineRef> }> }`: ONE per round,
    emitted right after the round's assistant row persists and before dispatch, listing EVERY call of the final message.
    The batch is what lets the reducer stamp `messageId` onto the round, add a call whose `toolCallStarted` never came,
    and drop a started call the final message doesn't hold (the nameless degenerate call).
  - `ToolCallFinished { message_id, call_id, outcome, subject }` (`outcome` replaces `ok`; `subject` is the refined
    one).
  - Terminal events carry `turn: Option<TurnStatsView>` (see the vocabulary): `Done`, `Failed`, and a new terminal
    `Stopped { turn }` for a cancel. The pre-turn `Failed` emits in `commands/agent/chat.rs` send `None`.
- **Terminal emission moves into `run_turn`.** `drive` stops emitting terminal events and returns what happened
  (answered with id/seq/stop, failed with kind + detail, or cancelled) plus the tally; `run_turn` appends
  `ConversationEvent::TurnFinished` when `tool_calls > 0` (on every outcome, cancel included; a persist problem here is
  logged and dropped, never a turn failure), then emits `Done` / `Failed` / `Stopped` with the stats. One place, so no
  early return can skip it. The tally gains `tool_calls`, `prompt_tokens`, `completion_tokens`, and `wall_ms`.
- **Persist the tool row, THEN emit `ToolCallFinished`**, so an expand never races the row.
- **`Done` carries the final round's `message_id`** (it already does): the answer round is text-only, so no
  `toolCallsResolved` stamps it, and the reducer stamps it from `done` (milestone 3a).
- **Repeat marks live**: `drive` keeps a per-turn `identity_key → LineRef` map and fills `repeat_of`.
- **The cancel registry** (`chat/cancel.rs`): fix the existing bug where a queued send's registration overwrites the
  running turn's token. ❌ Don't move registration under the lock: `commands/agent/chat.rs:228` deliberately registers
  BEFORE spawning so an immediate Stop finds it, and the wake registers before its lock too (`wake/runner.rs:194`).
  Instead give each registration a generation (or keep a per-conversation list) and make `cancel_turn` trip every live
  registration for the conversation; unregistering removes only its own. Set the turn's seq high-water mark UNDER the
  lock, in the shared `ChatRuntime` single-flight wrapper (`send` and `wake` in `runtime/mod.rs`), with a read-only
  `turn_in_flight_since_seq(id)` for 2b (decision 14).
- **`UserPersisted` carries the opener's view** (the persisted user row as a `MessageView`-shaped payload, or the fields
  the rail needs to build its user item), so a rail that reloaded before the first `End` can insert it.
- **The fake reports usage per round** (a scripted `AgentUsage` on each turn, default zero), so the summed-tokens test
  has real numbers to sum.
- `pnpm bindings:regen`, then the SMALLEST frontend change that compiles and keeps today's UI: the reducer derives the
  glyph from `outcome`, handles `stopped` as terminal (in `isTerminal`) by doing nothing yet, ignores
  `toolCallsResolved` for now, and finds a finished line by `callId` within the CURRENT bubble (today's lines carry no
  `messageId` yet; 3a moves to `LineRef`). History is untouched here (2b).

**Intentions**: every live fact the UI needs is on the stream, keyed by `(messageId, callId)`, with stats on every
terminal event from ONE place; the UI still looks like today.

**Landmines**:

- ❗ **Call-id collisions are the norm, not an edge.** Keep the fake's per-round `fake-call-{index}` as the regression
  fixture; ❌ don't "fix" the fake to mint unique ids.
- The crash contract (`chat/CLAUDE.md`): don't move any existing persist. `TurnFinished` is written after the turn's
  last row; a crash before it loses only the wall time (the backend fallback covers the rest).
- Cancel: `run_turn` has the connection after `drive` returns, so the cancel path can persist; keep the existing "cancel
  emits nothing else" semantics apart from the single `Stopped`.
- Wake turns run the same `run_turn`: they get subjects, stats, and the row. A quiet wake deletes its thread afterwards,
  row included; `wake/tests` stay green. `wake/runner.rs` / `watch.rs` may read terminal events: check.
- `analytics.rs` reads `TurnTally` / `TurnResult`; don't change what it reports.
- Runtime, stream, and views tests match old event shapes; update them, keeping each test's intent.

**Test plan** (red first):

- Two rounds that BOTH use `fake-call-0` (the fake as it is): each `toolCallFinished` lands on the right
  `(messageId, callId)`.
- Event order for a multi-tool turn: `toolCallStarted*` → `toolCallsResolved` → (`toolCallFinished` per call, each AFTER
  its row exists) → `done` with summed per-round tokens and the call count; an identical later call gets `repeat_of`.
- A round whose provider dropped a nameless call: `toolCallsResolved` omits it.
- `TurnFinished` rows: written for answered, failed, and cancelled turns that ran tools; not for a tool-less turn, whose
  terminal event carries `turn: None`. `stopped` carries stats on cancel.
- Cancel mid-dispatch: the round's remaining calls still dispatch and persist, then `stopped` arrives with stats that
  count them.
- The cancel registry: a queued send never replaces the running turn's token; Stop during `Queued` cancels the queued
  turn as well as the running one; unregistering one leaves the other; the seq mark reads back.

**DONE**: green Rust tests + clippy + svelte checks; bindings regenerated; the rail looks as before.

### 2b. Reload wiring: projection, turn ids, old-turn stats, the in-flight mark, the lazy result command (M)

**Scope**:

- **Projection** (logic in `agent/tools/line/` or a sibling, per "no business logic in commands"; `views.rs` only maps):
  `MessageBlock::ToolCall { call_id, tool, subject, outcome: Option<ToolOutcome>, repeat_of: Option<LineRef> }` (raw
  `arguments` dropped); tool-role rows project to no blocks. **Pairing**: results persist in call order (`turn.rs:270`),
  so the k-th occurrence of a `call_id` in an assistant row pairs with the k-th tool row carrying that `call_id` after
  it, before the next assistant or user row (the same rule the result command uses). Marks and stats are turn-scoped, so
  `ask_cmdr_get_conversation` loads rows from the OPENER of the page's first turn to the END of its last turn (❌ not
  the whole thread), builds the `LineRef → annotation` map and per-turn repeat marks, then projects the requested page.
  **One stats rule**: the projection ALWAYS attaches the turn's stats to the turn's LAST assistant view
  (`MessageView.turnStats: Option<TurnStatsView>`): from its `TurnFinished` row when there is one, else the fallback
  (`wallMs: null`) for a tool-running turn. ❌ No fallback for the turn holding rows after `turnInFlightSinceSeq`: it's
  still running, and its stats come from the live terminal event. The `TurnFinished` row itself projects like `Unknown`:
  an empty event view the fold drops.
- **Turn ids** (decision 16): every `MessageView` gains `turnId: Option<i64>` (the opener row's id; `None` before the
  first user row), and every event view `duringTurn: bool`.
- **`ConversationDetailView.turnInFlightSinceSeq: Option<i64>`** from the registry (decision 14).
- The `ask_cmdr_tool_result` command moved to milestone 5: registered with no caller, it fails `ipc-unused`, and an
  allowlist entry would be a loosening (David's call), so it lands with the expand that calls it.
- `pnpm bindings:regen`, then the smallest frontend change: history fills today's `path` from `Folder` / `Files`
  subjects and `ok` from `outcome`; `pathFromArguments` deleted.

**Intentions**: a reloaded thread carries the same facts the stream does; the UI still looks like today.

**Landmines**: `content_blocks` is backend-only (reasoning blob): the store helper returns typed parts, the command maps
to the view. Old-shape threads (raw args of any shape, colliding call ids, no `TurnFinished`) must load. The views tests
match the old block shapes; update them, keeping intent.

**Test plan** (red first): two rounds reusing `fake-call-0` annotate and fetch each round's own result; one assistant
row calling the same `call_id` twice pairs by occurrence; a result row on a different page than its call still
annotates; a page starting mid-turn gets the right `turnId`s and marks; event views carry `duringTurn` correctly
(mid-turn and post-turn decisions); an old-shape thread loads with fallback stats; a legacy `Unrecognized("")` call
loads; `turnInFlightSinceSeq` reflects the registry, and the in-flight turn gets NO fallback stats.

**DONE**: green Rust tests + clippy + svelte checks; bindings regenerated; the rail looks as before.

### 3a. The frontend turn model: rounds, live and reloaded (M)

**Scope**: `ask-cmdr-messages.ts`, `ask-cmdr-stream.svelte.ts`, `ask-cmdr-history.ts`, `ask-cmdr-state.svelte.ts`,
`ask-cmdr-trigger.svelte.ts`, `AskCmdrMessage.svelte` (only enough to render rounds), `AskCmdrRail.svelte`
(auto-scroll), and their tests. Milestone 4 does the lines themselves, but removing `ok` / `path` breaks
`AskCmdrToolLine.svelte` (~21-23, 45-53) and `rail.a11y.test.ts` (~486-488) now: adapt both minimally here (glyph from
`outcome`, path from a `Folder` / `Files` subject).

- **Shape**: assistant item
  `{ kind: 'assistant', id, turnId: number | null, rounds: Round[], thinking, stalled, streaming, stats: TurnStats | null }`
  (`turnId` from the views on reload, from `userPersisted.messageId` live; finalize leaves it alone),
  `Round = { messageId: number | null, text: string, calls: RailToolCall[] }`,
  `RailToolCall = { callId, tool, running, subject: ToolSubject | null, outcome: ToolOutcome | null, repeatOf: LineRef | null }`
  (`path` and `ok` go; `ok` derives from `outcome`). A pure `displaySegments(rounds)` yields `text` / `tools` segments,
  each round's text before its calls, merging adjacent tool groups when the round between has no text.
- **Live reducer**: `assistantStarted` opens a new round (`messageId: null`) in the current bubble (creating the bubble
  if needed). A round that has a `messageId` is SEALED: any streaming event (`textDelta`, `toolCallStarted`,
  `reasoningTick`) arriving when the current round is sealed opens a new round first, which is what makes a reload
  mid-turn (folded rounds all sealed) followed by live deltas come out right. `textDelta` appends to the CURRENT round's
  text even if its calls already started; `toolCallStarted` adds a line to the current round, keyed by `callId` within
  that round (idempotent: ERR-FRMAR); `toolCallsResolved` finds the round (the current one), stamps `messageId`, sets
  subjects and `repeatOf`, adds lines it lacks, drops started lines the batch doesn't list; `toolCallFinished` finds the
  line by `(messageId, callId)` and sets outcome + refined subject; `done` FIRST stamps the current round with its
  `messageId` (the text-only answer round has no `toolCallsResolved`), then finalizes; `done` / `failed` / `stopped`
  attach `turn` as `stats`.
- **History fold** (decision 16): items group by `turnId`; each turn folds into ONE assistant item: each assistant row
  is one round (`messageId` = its id, its text blocks joined, its calls); the stats ride the turn's last assistant view
  (2b's one stats rule), and a `TurnFinished` row's empty event view is dropped; event views with `duringTurn` are
  hoisted to just before that turn's bubble (matching live `showDecision`), the rest stay where they are; the bubble's
  `id` is the turn's LAST assistant row id. Result-less calls fold as settled (`running: false`, `outcome: null`) here;
  3b adds the in-flight case.
- **Page seam**: a pure `mergeAtSeam(older, newer)` merges the older page's last bubble with the newer list's first
  bubble exactly when their `turnId`s match: concatenate rounds (and the two halves of one `messageId` round if a page
  split it), keep the stats that exist, and take the bubble `id` from the NEWER half. Update the seam caveat in
  `DETAILS.md`.
- **Soft cap** (decision 12): `isOverSoftCap` counts user items + rounds + event items, keeping today's meaning (one per
  former bubble); the dropped `TurnFinished` view isn't an item, so it doesn't count. Pin it with a test.
- **`finalizeAssistant`**, when a terminal event settles a bubble (directly, or, from 3b on, by consuming a pending
  stop), the same way the fold would read the stored rows: in rounds WITH a `messageId`, flip still-running calls to
  settled (`running: false`, `outcome: null`); drop rounds whose `messageId` is still `null` entirely (they never
  reached `End`, so nothing was persisted: crash case a); drop empty rounds; set the bubble `id` to the last resolved
  round's id; pop an empty bubble. A nameless call vanishes with its unresolved round or was already dropped by
  `toolCallsResolved`. Tradeoff, written in `DETAILS.md`: when a stop ends in `stopped` or `failed`, the streaming
  round's text vanishes, because it was never stored and reload wouldn't show it.
- **Auto-scroll** (`AskCmdrRail.svelte` ~78-81, today `lastText.text`): depend on a derived content signature of the
  last bubble (round count, call count, last round's text length, stats present).

**Intentions**: the live bubble and the reloaded bubble for the same turn are structurally EQUAL. A test asserts deep
equality between the reducer's result for a scripted event sequence and the fold of the matching history, where the
sequence includes a round whose `textDelta`s interleave with its `toolCallStarted`s, two rounds reusing `fake-call-0`, a
mid-turn `proposalDecided` (hoisted), and a post-turn one (stays after). The comparison excludes live-only items: the
`contextTrimmed` notice and a failed turn's error item.

**Landmines**:

- `showDecision` and `applyContextTrimmed` splice items BEFORE the streaming bubble; keep the bubble LAST while
  streaming (`currentAssistant()` semantics).
- `renameApplied` lines are separate items; untouched.

**Test plan**: reducer tests for interleaved deltas, idempotent starts, resolve-adds-missing, resolve-drops-nameless,
colliding call ids across rounds, sealed rounds (reload mid-turn, then deltas open a new round), finalize parity with
the fold (settled `outcome: null` lines, unresolved rounds dropped), an answered turn keeping its final text (stamped by
`done`), `turnId` from `userPersisted`; fold tests for multi-round turns, wake openers, `duringTurn` hoisting, old
threads, stats on the last assistant view with the `TurnFinished` view dropped; `displaySegments` merge rules; seam
merge by `turnId` (incl. one round split across pages, id from the newer half); soft cap counting user items + rounds

- events; the live-equals-reload deep-equality test.

**DONE**: narration renders inline and in order, live and after reload; existing rail tests green (rewritten where they
asserted the old shape).

### 3b. Stop and in-flight reload (S–M)

**Scope**: `ask-cmdr-stream.svelte.ts`, `ask-cmdr-history.ts`, `ask-cmdr-trigger.svelte.ts`, and their tests.

- **Stop** (decision 15): `stopStreaming` finalizes the UI state (`streaming = false`, watchdog, review) but does NOT
  drop the unresolved streaming round, and records "pending stop → this bubble" for the conversation. Until a terminal
  arrives, these still apply without `adoptLiveTurn`: `toolCallsResolved` to the pending-stop bubble's last unresolved
  round, `toolCallFinished` and `proposalReady` by `LineRef`; everything else stays dropped. The next terminal of ANY
  kind consumes the pending stop: it settles that bubble (`done` keeps and stamps its last round; `stopped` / `failed`
  drop an unresolved round) and attaches stats, never touching `streaming`, the watchdog, or the review.
- **Wake stop**: a `stopped` with NO pending stop (the status corner stopping a wake) finalizes like `done`, including
  `openStagedRenameReview()`.
- **In-flight reload** (decision 14): result-less calls after `turnInFlightSinceSeq` fold as `running: true`; with no
  rows after the mark, the fold sets an explicit closed/no-reuse flag on the last bubble so `ensureAssistant()`
  (`ask-cmdr-stream.svelte.ts` ~301) opens a NEW one for the first live event. `loadConversation` calls the adopt path
  when the mark is present. `userPersisted` (carrying the opener's view) arrives AFTER `assistantStarted` and round 1's
  deltas, so it inserts the missing user item BEFORE the already-open bubble and patches that bubble's `turnId`.

**Intentions**: a Stop and a reload mid-turn both end in the same view a later reload shows.

**Landmines**: `handleTurnEvent` drops non-terminal events for a stopped thread; `stopped` must pass (`isTerminal`), and
the three settling events of decision 15 pass without `adoptLiveTurn`. A new send clears the stopped mark today
(`stoppedTurns.delete`); the pending-stop record must outlive that, so the old turn's terminal still finds its bubble.

**Test plan**: post-click `toolCallFinished` settling lines without re-adopting; a pending stop consumed by `done` /
`failed` / `stopped` alike; its stats landing on the stopped bubble even after a new send; a `stopped` with no pending
stop finalizing like `done` (review opens, no watchdog timeout); the in-flight mark (calls after it running, before it
settled, no rows after it → last bubble closed and the next live event opens a new bubble); `userPersisted` inserting a
missing user item BEFORE the already-open bubble and patching its `turnId`; the closed flag making the first live event
open a new bubble; `loadConversation` adopting.

**DONE**: Stop and reload mid-turn behave per decisions 14–15; checks green.

### 4. The tool line: subject, outcome, clickable paths, repeat mark, turn footer (M)

**Scope**: `AskCmdrToolLine.svelte`, `ask-cmdr-labels.ts`, a new `AskCmdrTurnStats.svelte` (named apart from the
existing thread-level `AskCmdrCostFooter.svelte`), `AskCmdrRail.svelte` + `routes/(main)/+page.svelte` (a `revealPath`
prop), the `askCmdr.*` catalog.

- Line = glyph + label (with its subject) + outcome. Labels stay keyed by TOOL (today's `TOOL_LABEL_KEYS` shape, running
  and done phrasing), with NEW keys taking subject params: "Listing {folder}", "Searched for {query} in {scope}", "Read
  {countText} files". Search has variants for no query, no scope, and several scope paths. An `Unparsed` or `null`
  subject falls back to today's param-less keys, which stay as they are (i18n rule: never add a placeholder to an
  existing value). A path mid-sentence renders through `<Trans>` with a `LinkButton` snippet. Outcome text per
  `ToolOutcome` kind: "{countText} items" / "showed {shown} of {totalText}", "{countText} matches" (+ "so far" for a
  lower bound), "nothing found" / "nothing found yet", "staged {countText}", "saved", "set aside to make room", and one
  line per `RefusalReason` (per `MemoryRefusalKind` for memory; reuse existing memory copy if any). A settled line with
  no outcome (decision 14) says no answer was recorded. Copy is drafts for David: friendly, short, never
  "error"/"failed" (`docs/style-guide.md`).
- **Clickable paths**: every path in a subject (`Folder.path`, `Files.firstPath`, `FocusedPane.folder`, each of
  `Search.scopePaths`, and `MemoryFile.absolute` when `Some`; else `MemoryFile.path` as plain text) renders as a
  `LinkButton` (plain text, ellipsized with the overflow tooltip). Click → `revealPath({ path, kind })`, a prop from
  `+page.svelte` like `readSelection`, implemented there with `resolveLocationOrToast` + `navigateToDirInBestPane`
  (folder) or `revealFileInBestPane` (file: parent dir + name), then `returnFocusToPane()` (`ask-cmdr-trigger.svelte.ts`
  ~116) so focus leaves the rail cleanly. Accessible name: "Show {path} in a pane".
- **Repeat mark**: a call with `repeatOf` shows a small "same as above" tag (with `aria-describedby`); the breaker's
  `Refused(RepeatedCall)` keeps its own outcome text.
- **Turn stats line** under the bubble, for any turn with stats (answered, failed, stopped): ONE ICU message (pluralized
  calls plus tokens, "{callsText} tool calls · {duration} · {tokensText} tokens") and a second key for the no-duration
  variant, via `formatMilliseconds` (or `formatDuration` on seconds) and `formatInteger`. Tokens use the cost meter's
  definition (prompt + completion per row), so turns add up to `AskCmdrCostFooter`'s thread figure. Hidden when stats
  are `null` or `toolCalls` is 0.
- **a11y**: lines lose `role="status"` / `aria-busy` (a 12-call turn would chatter); `aria-live` stays only on the LAST
  round's prose. Update the `rail.a11y.test.ts` comment that calls `role=status` load-bearing.
- `UNKNOWN_TOOL_KEYS` stays for `UnknownTool`; keep the "a missing label shows Working" test pinned.
- **Remove today's expand chevron and its path-only detail**: the path now sits in the label, and milestone 5 adds the
  real expand (the preview).

**Intentions**: points 1, 2, 6, 7 visible; every word is ours and localized; the line still reads as ONE calm row.

**Landmines**:

- ❗ A path `LinkButton` can't nest inside today's row `<button>`: the expand toggle and the path links become siblings.
  Mind `cmdr/prefer-ui-primitive` and the a11y tests (`rail.a11y.test.ts`).
- Long paths and queries: ellipsize, tooltip on overflow only.
- `cmdr/no-raw-user-facing-string`: every literal through `tString` / `<Trans>`.
- Retiring an old label key (if a tool's param-less key becomes unused): delete it from `en` and every locale in the
  same commit.
- Svelte `each` keys use `messageId:callId` (or the round index plus `callId` while `messageId` is null), never `callId`
  alone.

**Test plan**: label/outcome map tests per variant (exhaustive by type, plus a runtime test that every kind has
non-empty copy, plus the search variants and the `Unparsed` fallback); component tests for path clicks calling
`revealPath` with the right kind (including each scope path) and then `returnFocusToPane`, the repeat tag, the no-answer
state, and the stats line for answered and stopped turns, with and without duration; a11y tests for the new controls and
for lines no longer being status regions.

**DONE**: lines name their object and their outcome, paths navigate, repeats are marked, footers show; checks green.

### 5. Groups, the setting, and the expanded preview with Copy (M)

**Scope**: a new `AskCmdrToolGroup.svelte`, `AskCmdrToolLine.svelte` (expand), the setting in
`src/lib/settings/definitions/advanced.ts` + `types.ts` (+ a new Advanced card `settings.advanced.card.askCmdr` if no
fitting card exists; check `AdvancedSection.svelte`), the `askCmdr.*` and `settings.*` catalogs.

- **Group** = one `tools` display segment. Header: "Looked around: 6 folders, 2 searches" from per-category counts over
  its subjects (`Record<ToolSubject['kind'], Category>`, categories joined by `#lib/intl/list-format`, each an ICU
  plural); a subject-less line (a still-unresolved call) counts under a neutral category rather than breaking the
  header. A single-call segment renders the line alone, no group chrome. A group auto-opens when it first appears and
  stays open for the rest of the live turn; at the terminal event, `'collapse'` closes all that turn's groups and
  `'keep'` freezes each one's state. No "last segment" tracking.
- **The result command** (moved here from 2b, so it lands with its caller and `ipc-unused` stays green):
  `ask_cmdr_tool_result(conversation_id, message_id, call_id) -> Result<ToolResultDetailView, AskCmdrToolResultError>`
  in `commands/agent/conversations.rs`, registered in the `ipc.rs` manifest, plus its typed wrapper in
  `src/lib/tauri-commands/ask-cmdr.ts`. Worked out in 2b (built, tested, then set aside), so don't rediscover it:
  - Store helper `store::row_and_rest_of_turn(conn, conversation_id, message_id) -> Option<Vec<StoredMessage>>` in
    `agent/store/turns.rs`: first `SELECT seq FROM messages WHERE id = ?1 AND conversation_id = ?2` (the OWNERSHIP
    check: another thread's `message_id` is `None` → `NotFound`), then that row plus every row up to the next user-role
    row, through the file's `rows_where` helper.
  - Pure `tool_result_detail(rows, call_id) -> Option<ToolResultDetailView>` beside `project_page` in
    `agent/chat/message_view/thread.rs`: `calls_of(rows[0])`, then the SAME `pair_results(rows, 0, &calls)` the page
    uses (k-th occurrence of an id pairs with the k-th result carrying it, before the next assistant or user row), the
    FIRST call with that id; `None` when the row made no such call or it has no result. Returns
    `preview_of(&call.tool, result)` + `serde_json::to_string_pretty(&result.content)`.
  - `ToolResultDetailView { preview: ToolPreview, raw_json: String }` (specta, camelCase) lives in `thread.rs`,
    re-exported from `commands/agent/views.rs`; the error is a fieldless
    `AskCmdrToolResultError { NotFound, StoreUnavailable }` in `views.rs`. The command uses `db_path(&app)` (no store →
    `StoreUnavailable`) and `spawn_blocking`, logs only the store error, ❌ never the content.
  - Tests: found (preview + pretty JSON), unknown call id, a call with no result row, a refusal's `problem`, a set-aside
    row, two rounds reusing `fake-call-0` (each row reads its own), another thread's `message_id`. Update the `views.rs`
    header, `store/DETAILS.md` § Turn-scoped reads, and `chat/DETAILS.md` § What a reloaded page says (raw result's one
    door).
- **Expand on the line** (added here; milestone 4 removed today's chevron): see "Expanded line" below.
- **Setting** `askCmdr.toolGroupsOnAnswer`: `'collapse' | 'keep'` (default `'collapse'`), `section: ['Advanced']`, radio
  or select, label/description/keywords per the guide (then `node apps/desktop/scripts/sync-locale-keys.ts`). Pure
  frontend, read via `getSetting`: on the terminal event, that turn's groups collapse (`'collapse'`) or keep the open
  state the user left (`'keep'`). Reloaded groups start collapsed under both (decision 8), EXCEPT a group with a running
  line after a reload mid-turn: running wins, it starts open.
- **Open state lives outside the message list**: `AskCmdrRail.svelte` keys messages by INDEX (~164), so component-local
  state would jump between items on a prepend. Keep line-expanded and group-open state in a module-level `$state.raw`
  map keyed by stable ids: a line by `messageId:callId`, a group by its FIRST resolved line's `LineRef` (stable
  mid-turn, since calls only append). A group whose first round is still unresolved can't be toggled, so it needs no
  key. `mergeAtSeam` migrates the open-state entry from the newer half's first group key to the merged group's key (one
  map write; seams only happen on "load earlier" of settled turns). Not persisted. A manual toggle during the turn beats
  auto-open (the user's choice sticks until the terminal event applies the setting). Before auto-collapsing a group that
  holds focus, move focus to its toggle.
- **a11y**: the group header is NOT a live region. One visually hidden polite status per bubble announces a group's
  summary once, when it settles.
- **Expanded line**: on first expand, call the typed wrapper for `askCmdrToolResult(conversationId, messageId, callId)`,
  show a small spinner, then the preview: rows (name, plus size/line/snippet where present, paths clickable via
  `revealPath`), the truncation note ("showed the model {shown} of {totalText}", "so far" for a lower bound), the
  verbatim `problem` (labeled), or the set-aside note. Cache details in a plain `Map` (or `$state.raw`) keyed by
  `messageId:callId`, ❌ never inside the deep `$state` message list, and ❌ never log them. `NotFound` (a thread
  deleted underneath) shows a calm one-liner.
- **Copy**: `Button variant="secondary" size="mini"` (icon + "Copy") calling `copyToClipboard` from
  `#lib/tauri-commands` (as `CopyBox.svelte` does), with brief "Copied" feedback. ❌ No new clipboard plumbing.

**Intentions**: points 3 and 4. The group must stay quiet: one line when collapsed, no noise from 12 calls.

**Landmines**:

- Disable the expand toggle on a running line, a line with no `messageId` yet, and a settled line with `outcome: null`
  (no result row exists to fetch).
- The expand fetch can come back after the user switched threads; drop a stale answer (pattern: `latestThreadLoad`).
- The setting's `section` is its ONE home; Advanced auto-renders it, so ❌ don't hand-render it anywhere.
- The raw JSON can be large and private: render it nowhere, only copy it. If the clipboard refuses, say so calmly.
- i18n capture (`i18n-capture-ask-cmdr.ts`): a translator needs to see a group header and an expanded preview; add a
  capture state if cheap, else note it for the translator pass.

**Test plan**: group summary text per category mix (incl. a subject-less line); merged groups across a text-less round;
open/collapse transitions for both setting values on `done`, `failed`, and `stopped`, and on reload; a manual toggle
mid-turn surviving auto-open; focus moved to the toggle before an auto-collapse; open state surviving a "load earlier"
prepend, including a group the seam merges (its entry migrates to the merged key); a group's key staying put as calls
append mid-turn; the per-bubble status announcing once; expand disabled for running / unresolved / `outcome: null`
lines; expand success / NotFound / stale thread; the cache isn't reachable from `askCmdrState`; copy calls the clipboard
with the raw JSON; settings search finds the new row (the guide's two-way verify).

**DONE**: all eight points visible in the rail; settings row renders and is searchable; checks green.

### 6. E2E parity and the docs sweep (S–M)

**Scope**:

- The fake gains a `SayThenCall` scripted variant whose `End` message puts the `Text` part FIRST, then the calls (the
  genai `StreamEnd` order), while its deltas may interleave. The E2E fake is built in `agent/chat/session.rs`
  (`resolve_agent_llm` ~139, `scripted_fake_llm` ~204) and never sees the user's text at construction, so the fake picks
  its script per call from the user's own text, which is the LAST text part of the last user message in `respond`'s
  `messages` (`context.rs::build_messages` ~493 prepends an envelope text part to the latest user message), and its
  round index from the count of assistant messages after that user message (no internal counter, so a retry or a second
  thread can't desync it): one starting with a fixed marker (say `[tools]`) runs round 1 = narration + `app_state` +
  `list_volumes {}` + a hallucinated call (`CallTools` with `ToolId::Unrecognized("delete")`); round 2 = narration +
  `list_volumes {}` twice; round 3 = the answer. Anything else keeps the Say-only script.
- `ask-cmdr.spec.ts`: send the marker message and assert narration order, subjects, outcomes, the refused `delete` line,
  and repeat marks: the round-2 `list_volumes {}` calls BOTH mark as repeats of the round-1 call (identical args, first
  in the turn wins), even though round 2 reuses call ids from round 1. Assert the footer's call count and presence of a
  duration, ❌ not token numbers (the fake reports default usage: 0 tokens). Expand one line and assert the preview.
  Reload the window (or reopen the thread) and assert the SAME structure (point 8). Read `test/e2e-playwright/CLAUDE.md`
  first and run via `pnpm check desktop-e2e-playwright`.
- Docs sweep: `agent/chat/DETAILS.md` (event order, `toolCallsResolved`, terminal stats in `run_turn`,
  persist-then-emit, call-id collisions), `agent/tools/line/` C+D, `agent/memory/` (`MemoryRefusalKind`),
  `agent/store/DETAILS.md` (`TurnFinished`, `Unknown`, decision 11), the `commands/agent` view header (raw result on
  demand), `lib/ask-cmdr/DETAILS.md` (rounds, display segments, groups, seam merge, expand, setting, the soft cap
  counting user items + rounds + event items per decision 12), `docs/architecture.md` only if a map line is now wrong.

**Intentions**: lock live-equals-reload end to end, so a later change to either half fails a test, not a user.

**Landmines**: the fake runs as a LOCAL provider with its own prompt budget (`ask-cmdr/DETAILS.md` § The E2E fake-LLM
path); assert no gauge numbers. `app_state` and `list_volumes` read the E2E machine's real state; assert shape, not
values.

**DONE**: E2E green (twice isolated if it flakes, per the flake policy); docs current.

### 7. Live check against a real model, plus the narration nudge if needed (S)

**Scope**: run a real multi-tool turn in a dev app and check the result. Read `apps/desktop/DETAILS.md` § Running and §
MCP control first. `pnpm dev --worktree ask-cmdr-tool-transparency`, configure Anthropic with
`secret ANTHROPIC_API_KEY_CMDR_DEV` through the app's normal key path (`ai/DETAILS.md`; never write the key into the
repo or a log), allow cloud AI, pick a current Claude model (verify the id from the provider's models endpoint, not
memory), and send prompts that force several calls, for example "Which of my drives has the most free space, and what
are the three biggest folders in my Downloads?" and "Find PDFs modified this month and tell me what the newest one is
about."

Check, with screenshots of STABLE states only (after the terminal event; David QAs live motion himself):

1. Every line names its object and its outcome; paths navigate a pane.
2. Groups summarize and collapse per the setting (try both values).
3. Expand shows an honest preview; Copy yields the raw JSON, which matches the stored row.
4. Narration appears between groups, in order. **If the model emits none**, add ONE line to `agent/prompts/system.md`
   (for example "Before a batch of tool calls, say in one short sentence what you're checking."), re-measure
   `budget::FIXED_PROMPT_OVERHEAD_TOKENS` (pinned by `context/cost_tests.rs`), and re-run. Skip the nudge if narration
   already shows; record which in `agent/prompts/DETAILS.md`.
5. The footer's tokens equal the sum of the turn's rows; wall time looks right. Stop one turn mid-way: its footer shows.
6. Quit, relaunch, reopen the thread: identical view.

**Landmines**: real spend: keep it to a handful of turns. Tear the dev data dir down only via
`~/.claude/scripts/remove-worktree.sh` at the end of the whole effort, not here. A model id that 404s is a stale id, not
a bug.

**DONE**: a short report with screenshots' paths (scratchpad), what passed, and any copy that reads wrong for David.

## Invariants (the conformance review checks these)

1. Every wire field this effort adds is a typed enum, number, path, the model's own text, file data (`PreviewRow.name` /
   `snippet`), or the copy-only `rawJson`; no Rust-authored English reaches UI copy except the verbatim `problem` in the
   expanded preview.
2. No outcome, refusal, or state is classified by string-matching; outcomes read our own result keys or typed tokens
   (`MemoryRefusalKind::from_token`).
3. Subject, outcome, preview, repeat mark, and turn stats (incl. old-turn fallbacks) are derived in Rust by ONE set of
   pure functions used by both the live path and the history projection.
4. A line is identified by `(messageId, callId)` in every event, lookup, map, mark, command, and `each` key.
5. The repeat breaker and the repeat mark share one identity key (`AgentToolCall::identity_key`), scoped to one turn.
6. A tool result row is persisted before its `ToolCallFinished` is emitted.
7. Live turn stats come only from `run_turn` and ride the terminal event (`done`, `failed`, `stopped`); a terminal event
   from outside a turn carries `turn: None`; a tool-running turn writes `TurnFinished` whatever its outcome.
8. The raw tool result crosses IPC only through `ask_cmdr_tool_result`, on the user's expand; never in the stream or the
   history page; never a reasoning part; never logged; never cached in deep `$state`.
9. Nothing the model sees changes, except an optional one-line system-prompt nudge with its overhead re-measured.
10. A thread written before this work loads and renders; every derivation is total; an unknown event kind keeps its row
    as an empty event view.
11. The live bubble and the reloaded bubble for the same turn are structurally equal (pinned by a unit test and the
    E2E), including interleaved deltas, colliding call ids, and a mid-turn decision.
12. Narration renders only through `renderAssistantMarkdown`; every path, name, query, snippet, and JSON renders as
    plain text.
13. Paths navigate through the existing `navigate-and-select` primitives, ❌ no new navigation path, and only subject
    paths (never a raw scope string) reach `revealPath`.
14. The collapse setting lives only in Settings > Advanced and is read on the frontend; reloaded groups start collapsed
    unless a line in them is running.
15. No migration-ladder step was added; `TurnFinished` is an event row.
16. The soft-cap nudge counts user items + rounds + event items, keeping today's meaning.
17. New strings are English-only with `@key` descriptions; no translations were written; no existing key's value gained
    a placeholder; retired keys left every catalog in the same commit.
18. Lines are not live regions; one polite status per bubble announces a settled group once.
19. UI open/expanded state lives in a module-level map keyed by stable ids, never by list index.
20. Turn edges come from the backend (`turnId`, `duringTurn`, `turnInFlightSinceSeq`); the frontend never guesses them.
21. After a local stop, the next terminal of any kind settles the stopped bubble only; a `stopped` with no local stop
    finalizes like `done`.
