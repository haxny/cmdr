/**
 * ⌘⏎ / ⌘S in the Tags window: the write at once (`TagEditorState.writeNow`, the Write dialog's own write without its
 * list), what the status line says about it, and the would-drop choice it may ask over the panes (`WouldDropDialog`).
 * `TagsContent` holds one. It waits first for an Album or clipboard edit that hasn't reached the state yet
 * (`editsSent`), so a paste or a box's commit pressed just before is written, never left out.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import type { ContentKind } from '#lib/ipc/bindings.js'
import { getFirstShiftShortcutReactive } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
import type { TagApplyReport, TagEditorView } from '#lib/tauri-commands/index.js'
import { countsOf, outcomeOf, refusalOf, reviewFirstPart } from './apply-outcome'
import type { TagEditorState, WriteNowResult } from './tag-editor-state.svelte'
import { answerWriteAnyway } from './write-anyway'
import { KIND_LABELS } from './write-kind'

/** How long the status line keeps a write's summary, unless an edit comes first. */
const SUMMARY_SHOWN_MS = 10_000

/** One sentence of a write's summary in the status line. */
export interface SummaryPart {
  key: MessageKey
  params?: Record<string, string | number>
}

export interface WriteSummary {
  parts: SummaryPart[]
  role: 'status' | 'alert'
}

/** Files a write at once would make lose tag data ask first, over the panes. */
export interface DropChoice {
  reports: TagApplyReport[]
  kind: ContentKind
  /** The view the choice was asked for: an edit since then lists again. */
  seen: TagEditorView | null
  answering: boolean
  sideFiles: boolean
}

export interface WriteNowDeps {
  tags: TagEditorState
  /** Stops a playing track before the files are written. */
  stopPlayer: () => void
  /** Opens the Write dialog's list, continuing these applies of the press. */
  openReview: (continuing?: TagApplyReport[]) => void
  /** Words a short notice in the status line. */
  say: (key: MessageKey) => void
  /** Resolves `true` once an edit started but not yet sent has reached the state, `false` when one stayed out. */
  editsSent: () => Promise<boolean>
}

export interface WriteNowStatus {
  /** A write at once is on its way: a second press does nothing, and the status line says "Writing tags…". */
  readonly writingNow: boolean
  /** The last write's summary while the view it left is shown and for 10 s, else `null`. */
  readonly shownSummary: WriteSummary | null
  readonly dropChoice: DropChoice | null
  writeAtOnce(): Promise<void>
  /** "Skip these files": the would-drop files stay as they are, counted as not written. */
  skipDropped(choice: DropChoice): void
  /** "Write anyway" (`answerWriteAnyway`, shared with the Write dialog, so both write and re-list alike). */
  writeAnyway(choice: DropChoice): Promise<void>
  /** Stops the summary's timer (the window unmounts). */
  destroy(): void
}

export function createWriteNowStatus(deps: WriteNowDeps): WriteNowStatus {
  const { tags } = deps
  let writingNow = $state(false)
  let summary = $state.raw<(WriteSummary & { view: TagEditorView | null }) | null>(null)
  let dropChoice = $state.raw<DropChoice | null>(null)
  let summaryTimer: ReturnType<typeof setTimeout> | undefined

  function showSummary(parts: SummaryPart[], role: 'status' | 'alert' = 'status'): void {
    clearTimeout(summaryTimer)
    summary = { parts, role, view: tags.view }
    summaryTimer = setTimeout(() => {
      summary = null
    }, SUMMARY_SHOWN_MS)
  }

  /** How many files were written, and how many weren't (pointing at Review, which lists them). */
  function showDone(written: number, notWritten: number, after: SummaryPart[] = []): void {
    const count = { count: written, countText: formatInteger(written) }
    const done: SummaryPart =
      notWritten === 0
        ? { key: 'multiRename.tags.write.doneSummary', params: count }
        : {
            key: 'multiRename.tags.write.doneWithProblems',
            params: {
              ...count,
              notWritten,
              notWrittenText: formatInteger(notWritten),
              shortcut: getFirstShiftShortcutReactive('tags.review') ?? '',
            },
          }
    showSummary([done, ...after], after.length > 0 ? 'alert' : 'status')
  }

  /** The summary of every apply of one press (the first, and a "Write anyway"), each file once. */
  function showOutcome(reports: TagApplyReport[], after: SummaryPart[] = []): void {
    const { written, notWritten } = countsOf(outcomeOf(reports))
    showDone(written, notWritten, after)
  }

  function answerWrite(res: WriteNowResult): void {
    switch (res.kind) {
      case 'done':
        showDone(res.written, res.failed + res.changedOnDisk)
        break
      case 'wouldDrop':
        dropChoice = {
          reports: [res.report],
          kind: res.writtenAs,
          seen: tags.view,
          answering: false,
          sideFiles: res.sideFiles,
        }
        break
      case 'refused': {
        const { noneWritten, reason } = refusalOf(res.error.kind)
        const parts: SummaryPart[] = [{ key: reason }]
        showSummary(noneWritten ? [{ key: 'multiRename.tags.write.refused' }, ...parts] : parts, 'alert')
        break
      }
      case 'listFailed':
        showSummary([{ key: 'multiRename.tags.write.listFailed' }], 'alert')
        break
      case 'reviewFirst':
        // A removal or a side-file replacement is written only from the list that names it; it continues any apply.
        deps.openReview(res.after)
        showSummary([reviewFirstPart(res, tString(KIND_LABELS[res.writtenAs]))])
        break
      case 'changed':
        // An edit landed between the list and the write: nothing unlisted is written.
        deps.openReview()
        showSummary([{ key: 'multiRename.tags.write.listedAgain' }])
        break
      case 'nothing':
        showSummary([{ key: 'multiRename.tags.write.nothingToWrite' }])
        break
      case 'stillBusy':
        showSummary([{ key: 'multiRename.tags.write.stillBusy' }], 'alert')
        break
    }
  }

  return {
    get writingNow() {
      return writingNow
    },
    get shownSummary() {
      return summary !== null && summary.view === tags.view ? summary : null
    },
    get dropChoice() {
      return dropChoice
    },
    async writeAtOnce() {
      if (writingNow || tags.writing) return
      writingNow = true
      summary = null
      try {
        // A paste or a box's commit pressed just before goes out first, so the write's list holds it.
        const res: WriteNowResult = (await deps.editsSent())
          ? await tags.writeNow({ onWriting: deps.stopPlayer })
          : { kind: 'stillBusy' }
        answerWrite(res)
      } finally {
        writingNow = false
      }
    },
    skipDropped(choice) {
      dropChoice = null
      showOutcome(choice.reports)
    },
    async writeAnyway(choice) {
      if (choice.answering) return
      dropChoice = { ...choice, answering: true }
      const answer = await answerWriteAnyway(tags, choice, deps.stopPlayer)
      switch (answer.kind) {
        case 'stillBusy':
          dropChoice = { ...choice, answering: false }
          deps.say('multiRename.tags.write.stillBusy')
          break
        case 'changed':
          // An edit landed while the choice showed: nothing unlisted is written. The list opens, continuing this
          // press, so its write's Undo reverses the first apply too.
          dropChoice = null
          deps.openReview(choice.reports)
          break
        case 'written':
          dropChoice = null
          showOutcome(answer.reports)
          break
        case 'refused':
          dropChoice = null
          showOutcome(choice.reports, [{ key: 'multiRename.tags.write.stopped' }])
          break
      }
    },
    destroy() {
      clearTimeout(summaryTimer)
    },
  }
}
