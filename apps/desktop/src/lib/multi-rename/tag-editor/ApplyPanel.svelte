<script lang="ts">
    /**
     * Writing the Tags window's edits into the files. "Write tags…" first lists what will change, per file; only then
     * does "Write tags" write. Fields that still carry diacritics are named with a one-click fix, never stripped
     * silently. Files that would lose tag data ask (`WouldDropDialog`); every file left unwritten is listed with why
     * (`ApplyOutcome.svelte`). Every press answers here, next to the buttons: it acts, shows that it's waiting for the
     * last change ("Finishing the last change…"), or says why it can't (`notice`), never ends without a word.
     * "Undo tag changes" reverses the last write until the next one; a file changed since that
     * write (written again, or another file renamed onto its name) is left alone and counted apart. It is the body
     * of the Tags window's Write dialog (`WriteDialog`): ⌘S runs `start()`, ⌘Z runs `undo()`.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { formatUnitList } from '#lib/intl/list-format.js'
    import { joinSentences } from '#lib/intl/sentence-join.js'
    import type { ContentKind, KindRemoval, TagField } from '#lib/ipc/bindings.js'
    import type { FileChangeView, TagApplyReport, TagEditorView } from '#lib/tauri-commands/index.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import Button from '#lib/ui/Button.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import ToggleGroup from '#lib/ui/ToggleGroup.svelte'
    import { confirmDialog } from '#lib/utils/confirm-dialog.js'
    import ApplyOutcome from './ApplyOutcome.svelte'
    import WouldDropDialog from './WouldDropDialog.svelte'
    import { outcomeOf, writeAnywayRequest, type ApplyOutcome as Outcome } from './apply-outcome'
    import { FIELD_LABELS } from './field-labels'
    import { undoableCount } from './last-tag-apply.svelte'
    import type { TagEditorState, TagStateError, TagUndoResult } from './tag-editor-state.svelte'
    import { DEFAULT_WRITE_KIND, WRITE_KINDS, loadWriteKind, saveWriteKind } from './write-kind'
    import { WAITING_SHOWN_AFTER_MS, WRITE_WAIT_LIMIT_MS } from './write-wait'

    interface Props {
        tags: TagEditorState
        /** Called just before a write starts (the Tags window stops a playing track: lofty rewrites the file it streams). */
        onWriting?: () => void
        /** Whether a write is on its way (the dialog can't be closed under it). */
        onBusy?: (busy: boolean) => void
    }

    const { tags, onWriting, onBusy }: Props = $props()

    type Step =
        | { step: 'idle' }
        /** A press waits for the calls still out (an edit a blur just sent) before it lists or writes. */
        | { step: 'waiting' }
        | { step: 'checking' }
        /**
         * The list shown, the kind it was made for (the write uses that one, so it does just what the list says), and
         * the view it was made from: an edit since then lists again before any write. `earlier` holds this press's
         * applies so far (a re-list after the would-drop choice), so the next write continues them.
         */
        | {
              step: 'review'
              changes: FileChangeView[]
              kind: ContentKind
              seen: TagEditorView | null
              earlier: TagApplyReport[]
          }
        | { step: 'writing' }
        /**
         * This press's applies; the last one's would-drop files ask. `seen` is the view after it: an edit made while
         * the choice shows lists again.
         */
        | {
              step: 'wouldDrop'
              reports: TagApplyReport[]
              kind: ContentKind
              seen: TagEditorView | null
              answering: boolean
          }
        | { step: 'done'; outcome: Outcome }
        /** The write was refused: `noneWritten` when that came before any file was touched, and why. */
        | { step: 'refused'; noneWritten: boolean; reason: MessageKey }
        | { step: 'undone'; restored: number; left: number; changed: number }

    let current = $state.raw<Step>({ step: 'idle' })
    /** Why the last press didn't go on, worded next to the buttons; cleared by the next press. */
    let notice = $state<MessageKey | null>(null)
    /** A press is being answered: set at once, so a second click or ⌘⏎ before any step shows starts nothing. */
    let pressing = $state(false)

    /** Runs a press unless one is already being answered. */
    async function oncePressed(run: () => Promise<void>): Promise<void> {
        if (pressing) return
        pressing = true
        try {
            await run()
        } finally {
            pressing = false
        }
    }
    let kind = $state<ContentKind>(DEFAULT_WRITE_KIND)
    /** The remembered kind arrives once; a pick, or a list already made, before it lands wins. */
    let kindChosen = false
    void loadWriteKind().then((remembered) => {
        if (!kindChosen) kind = remembered
    })

    function pickKind(value: string): void {
        const next = WRITE_KINDS.find((k) => k === value)
        if (next === undefined || kindLocked) return
        kindChosen = true
        kind = next
        saveWriteKind(next)
    }

    const view = $derived(tags.view)
    const canWrite = $derived(view !== null && view.editable)
    const kindOptions = $derived([
        { value: 'audiobook', label: tString('multiRename.tags.kind.audiobook') },
        { value: 'music', label: tString('multiRename.tags.kind.music') },
    ])
    const labelOf = (of: ContentKind): string => kindOptions.find((option) => option.value === of)?.label ?? ''
    /** While a list or a write is on its way the kind stays as it was asked for. */
    const kindLocked = $derived(current.step === 'waiting' || current.step === 'checking' || current.step === 'writing')
    const fieldNames = (fields: readonly TagField[]): string[] =>
        fields.map((field) => tString(FIELD_LABELS[field]))

    const REMOVAL_KEYS = { conductor: 'multiRename.tags.write.removesConductor' } as const satisfies Record<
        KindRemoval,
        string
    >

    function changeText(change: FileChangeView): string {
        const parts = fieldNames(change.fields)
        if (change.cover) parts.push(tString('multiRename.tags.write.cover'))
        if (change.extras) parts.push(tString('multiRename.tags.write.extras'))
        for (const removal of change.removes) parts.push(tString(REMOVAL_KEYS[removal]))
        return tString('multiRename.tags.write.fileChanges', { fileName: change.fileName, fields: formatUnitList(parts) })
    }

    /**
     * Waits for the calls still out, showing "Finishing the last change…" when it takes a moment (`showWaiting`), up to
     * `WRITE_WAIT_LIMIT_MS`. A wait that gives up (a stalled mount) says so and leaves the step as it was, so the same
     * button can be pressed again.
     */
    async function callsAnswered(showWaiting = true): Promise<boolean> {
        notice = null
        const before = current
        const timer = showWaiting
            ? setTimeout(() => {
                  current = { step: 'waiting' }
              }, WAITING_SHOWN_AFTER_MS)
            : undefined
        try {
            const answered = await tags.settled(WRITE_WAIT_LIMIT_MS)
            if (!answered) notice = 'multiRename.tags.write.stillBusy'
            return answered
        } finally {
            clearTimeout(timer)
            if (current.step === 'waiting') current = before
        }
    }

    /** Lists what a write will change. `earlier` carries this press's applies so far through a re-list. */
    async function review(earlier: TagApplyReport[] = []): Promise<void> {
        // A call still out after the wait: list nothing rather than a list that may miss it.
        if (!(await callsAnswered())) return
        const listedAs = kind
        kindChosen = true
        current = { step: 'checking' }
        const res = await tags.changes(listedAs)
        if (res.ok) current = { step: 'review', changes: res.value, kind: listedAs, seen: tags.view, earlier }
        else {
            current = endedStep(earlier)
            notice = 'multiRename.tags.write.listFailed'
        }
    }

    /** Writes what `reviewed` listed. A list with nothing in it writes nothing (and so keeps the last Undo). */
    async function write(reviewed: Extract<Step, { step: 'review' }>): Promise<void> {
        const { changes, kind: listedAs, seen, earlier } = reviewed
        if (changes.length === 0) return
        // A call still out after the wait: write nothing; the list stays for another press.
        if (!(await callsAnswered())) return
        if (tags.view !== seen) {
            await review(earlier)
            return
        }
        current = { step: 'writing' }
        onWriting?.()
        const res = await tags.apply({ kind: listedAs, allowFrameLoss: false, skip: [] }, { after: earlier })
        if (!res.ok) {
            current = refusedStep(res.error.kind, earlier)
            return
        }
        const reports = [...earlier, res.value]
        if (res.value.wouldDrop.length > 0) {
            current = { step: 'wouldDrop', reports, kind: listedAs, seen: tags.view, answering: false }
        }
        else current = endedStep(reports)
    }

    /** Where a press ends: what its applies did, or nothing to show when it made none. */
    function endedStep(reports: TagApplyReport[]): Step {
        return reports.length > 0 ? { step: 'done', outcome: outcomeOf(reports) } : { step: 'idle' }
    }

    /**
     * A refused write, worded. These refusals come before any file is touched; another (a task that died) may come
     * after some were written, so it doesn't claim none were. This press's earlier applies stay listed.
     */
    function refusedStep(why: TagStateError['kind'], earlier: TagApplyReport[]): Step {
        if (earlier.length > 0) {
            notice = 'multiRename.tags.write.stopped'
            return endedStep(earlier)
        }
        switch (why) {
            case 'undo':
            case 'noAppDataDir':
                return { step: 'refused', noneWritten: true, reason: 'multiRename.tags.undoStoreUnavailable' }
            case 'readOnly':
                return { step: 'refused', noneWritten: true, reason: 'multiRename.tags.readOnly' }
            case 'noSession':
            case 'notOpen':
                return { step: 'refused', noneWritten: true, reason: 'multiRename.tags.write.sessionGone' }
            default:
                return { step: 'refused', noneWritten: false, reason: 'multiRename.tags.write.stopped' }
        }
    }

    /** Every path the write knows, so "Write anyway" can skip all but the would-drop ones. */
    function knownPaths(first: TagApplyReport): string[] {
        return [
            ...(tags.view?.tracks.map((track) => track.path) ?? []),
            ...first.written,
            ...first.failed.map((row) => row.path),
            ...first.changedOnDisk,
            ...first.wouldDrop.map((row) => row.path),
        ]
    }

    async function writeAnyway(choice: Extract<Step, { step: 'wouldDrop' }>): Promise<void> {
        const { reports, kind: listedAs, seen } = choice
        const last = reports.at(-1)
        if (last === undefined) return
        current = { ...choice, answering: true }
        if (!(await callsAnswered(false))) {
            // A call still out after the wait: write nothing, and the choice asks again.
            current = { ...choice, answering: false }
            return
        }
        if (tags.view !== seen) {
            // Something was edited while the choice showed: nothing is written that wasn't listed. The applies so
            // far go along, so the next write continues them and their Undo isn't lost.
            await review(reports)
            return
        }
        const request = writeAnywayRequest(listedAs, last.wouldDrop, knownPaths(last))
        onWriting?.()
        const res = await tags.apply(request, { after: reports })
        // A refused "Write anyway" leaves those files unwritten, as a skip does, and says it stopped.
        if (res.ok) current = endedStep([...reports, res.value])
        else current = refusedStep(res.error.kind, reports)
    }

    /** The undo question: how many files go back, that a file changed since is left alone, and unwritten edits are lost. */
    function undoQuestion(count: number): string {
        const message = tString('multiRename.tags.undo.message', { count, countText: formatInteger(count) })
        return tags.view?.dirty === true
            ? joinSentences([message, tString('multiRename.tags.undo.unwrittenLost')])
            : message
    }

    /** Undo tag changes (and ⌘Z): asks first. Resolves whether it undid anything (`false`: nothing to undo, or no). */
    export async function undo(): Promise<boolean> {
        await tags.settled()
        const last = tags.undoable
        if (last === null) return false
        const sure = await confirmDialog(
            undoQuestion(undoableCount(last)),
            tString('multiRename.tags.undo.title'),
            tString('multiRename.tags.undo.confirm'),
        )
        if (!sure) return false
        current = undoneStep(await tags.undo())
        return true
    }

    /** What an undo did: the files back as they were, and those still left (a failure leaves them undoable). */
    function undoneStep(res: TagUndoResult): Step {
        if (res.ok) {
            const left = res.value.missing.length + res.value.failed.length
            return { step: 'undone', restored: res.value.restored.length, left, changed: res.value.changed.length }
        }
        const left = tags.undoable === null ? 0 : undoableCount(tags.undoable)
        return { step: 'undone', restored: res.restored.restored.length, left, changed: res.restored.changed.length }
    }

    async function removeDiacritics(fields: TagField[]): Promise<void> {
        await tags.settled()
        await tags.tool({ kind: 'stripDiacritics', fields })
    }

    async function reload(): Promise<void> {
        const sure =
            tags.view?.dirty !== true ||
            (await confirmDialog(
                tString('multiRename.tags.reload.message'),
                tString('multiRename.tags.reload.title'),
                tString('multiRename.tags.reload.confirm'),
            ))
        if (!sure) return
        await tags.reload()
        current = { step: 'idle' }
    }

    /** The sheet's Start: lists the changes, and from the list writes them. */
    export async function start(): Promise<void> {
        // Read-only: the tab says so at its top, and the button is disabled.
        if (!canWrite) return
        await oncePressed(async () => {
            if (current.step === 'review') await write(current)
            else if (['idle', 'done', 'refused', 'undone'].includes(current.step)) await review()
        })
    }

    /** How many files the write it shows left unwritten (failed, changed on disk, skipped); 0 when it shows none. */
    export function notWritten(): number {
        if (current.step !== 'done') return 0
        const { failed, changedOnDisk, skipped } = current.outcome
        return failed.length + changedOnDisk.length + skipped.length
    }

    $effect(() => {
        onBusy?.(current.step === 'writing' || (current.step === 'wouldDrop' && current.answering))
    })

    const undoable = $derived(tags.undoable !== null && current.step !== 'writing' && current.step !== 'wouldDrop')
</script>

<section class="apply" aria-label={tString('multiRename.tags.write.confirm')}>
    {#if view !== null && view.diacritics.length > 0}
        <p class="warning">
            {tString('multiRename.tags.diacritics.warning', { fields: formatUnitList(fieldNames(view.diacritics)) })}
            <Button
                size="mini"
                disabled={!canWrite}
                onclick={() => { void removeDiacritics([...view.diacritics]) }}
            >
                {tString('multiRename.tags.diacritics.fix')}
            </Button>
        </p>
    {/if}

    {#if current.step === 'review'}
        {@const reviewed = current}
        {@const { changes, earlier } = reviewed}
        {#if earlier.length > 0}
            <ApplyOutcome
                outcome={outcomeOf(earlier)}
                someUndoable={tags.undoable !== null}
                onReload={() => { void reload() }}
            />
        {/if}
        {#if changes.length === 0}
            <p>{tString('multiRename.tags.write.nothing')}</p>
        {:else}
            <p>
                {tString('multiRename.tags.write.summaryAs', {
                    kind: labelOf(reviewed.kind),
                    count: changes.length,
                    countText: formatInteger(changes.length),
                })}
            </p>
            <ul class="changes" aria-label={tString('multiRename.tags.write.changeLabel')}>
                {#each changes as change (change.path)}
                    <li>{changeText(change)}</li>
                {/each}
            </ul>
        {/if}
        <div class="actions">
            <Button variant="primary" disabled={!canWrite || changes.length === 0} keyHint={{ commandId: 'multiRename.rename' }} onclick={() => { void oncePressed(() => write(reviewed)) }}>
                {tString('multiRename.tags.write.confirm')}
            </Button>
            <Button onclick={() => { notice = null; current = endedStep(earlier) }}>{tString('multiRename.tags.write.back')}</Button>
        </div>
    {:else if current.step === 'wouldDrop'}
        {@const choice = current}
        <WouldDropDialog
            rows={choice.reports.at(-1)?.wouldDrop ?? []}
            busy={choice.answering}
            onWriteAnyway={() => { void oncePressed(() => writeAnyway(choice)) }}
            onSkip={() => { current = endedStep(choice.reports) }}
        />
    {:else}
        {#if current.step === 'checking'}
            <p role="status">{tString('multiRename.tags.write.checking')}</p>
        {:else if current.step === 'writing'}
            <p role="status">{tString('multiRename.tags.write.writing')}</p>
        {:else if current.step === 'done'}
            <ApplyOutcome
                outcome={current.outcome}
                someUndoable={tags.undoable !== null}
                onReload={() => { void reload() }}
            />
        {:else if current.step === 'waiting'}
            <p class="waiting" role="status"><Spinner size="sm" />{tString('multiRename.tags.write.waiting')}</p>
        {:else if current.step === 'refused'}
            <p role="alert">
                {#if current.noneWritten}{tString('multiRename.tags.write.refused')}{/if}
                {tString(current.reason)}
            </p>
        {:else if current.step === 'undone'}
            {@const { restored, left, changed } = current}
            <p role="status">
                {#if restored > 0}{tString('multiRename.tags.undo.done', { count: restored, countText: formatInteger(restored) })}{/if}
                {#if left > 0}{tString('multiRename.tags.undo.notRestored', { count: left, countText: formatInteger(left) })}{/if}
                {#if changed > 0}{tString('multiRename.tags.undo.changedSince', { count: changed, countText: formatInteger(changed) })}{/if}
            </p>
        {/if}
        <div class="actions">
            <ToggleGroup
                semantics="toggles"
                value={kind}
                options={kindOptions}
                ariaLabel={tString('multiRename.tags.kind.label')}
                disabled={!canWrite || kindLocked}
                onChange={pickKind}
            />
            <Button
                variant="primary"
                disabled={!canWrite || kindLocked || pressing}
                keyHint={{ commandId: 'multiRename.rename' }}
                onclick={() => { void start() }}
            >
                {tString('multiRename.tags.write.review')}
            </Button>
        </div>
    {/if}
    {#if notice !== null}
        <p class="notice" role="alert">{tString(notice)}</p>
    {/if}
    {#if undoable}
        <div class="actions">
            <Button tooltipContent={{ text: tString('multiRename.tags.undo.tooltip') }} onclick={() => { void undo() }}>
                {tString('multiRename.tags.undo.button')}
            </Button>
        </div>
    {/if}
</section>

<style>
    .apply {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    p {
        margin: 0;
    }

    .waiting {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .notice {
        color: var(--color-warning-text);
    }

    .warning {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        color: var(--color-warning-text);
    }

    /* A long book's list scrolls inside the dialog; the buttons under it stay in view. */
    .changes {
        margin: 0;
        padding-left: var(--spacing-lg);
        overflow-wrap: anywhere;
        max-height: 40vh;
        overflow: auto;
    }

    .actions {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        flex-wrap: wrap;
    }
</style>
