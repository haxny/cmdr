<script lang="ts">
    /**
     * Writing the Tags tab's edits into the files. "Write tags…" first lists what will change, per file; only then
     * does "Write tags" write. Fields that still carry diacritics are named with a one-click fix, never stripped
     * silently. Files that would lose tag data ask (`WouldDropDialog`); every file left unwritten is listed with why
     * (`ApplyOutcome.svelte`). "Undo tag changes" reverses the last write until the next one, and says it overwrites
     * any change made to those files since. The sheet's Start (⌘⏎) comes here while the Tags tab shows: `start()`.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { formatUnitList } from '#lib/intl/list-format.js'
    import { joinSentences } from '#lib/intl/sentence-join.js'
    import type { ContentKind, TagField } from '#lib/ipc/bindings.js'
    import type { FileChangeView, TagApplyReport, TagEditorView } from '#lib/tauri-commands/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToggleGroup from '#lib/ui/ToggleGroup.svelte'
    import { confirmDialog } from '#lib/utils/confirm-dialog.js'
    import ApplyOutcome from './ApplyOutcome.svelte'
    import WouldDropDialog from './WouldDropDialog.svelte'
    import { outcomeOf, writeAnywayRequest, type ApplyOutcome as Outcome } from './apply-outcome'
    import { FIELD_LABELS } from './field-labels'
    import { undoableCount } from './last-tag-apply.svelte'
    import type { TagEditorState, TagUndoResult } from './tag-editor-state.svelte'

    interface Props {
        tags: TagEditorState
    }

    const { tags }: Props = $props()

    type Step =
        | { step: 'idle' }
        | { step: 'checking' }
        /**
         * The list shown, and the view it was made from: an edit since then lists again before any write. `earlier`
         * holds this press's applies so far (a re-list after the would-drop choice), so the next write continues them.
         */
        | { step: 'review'; changes: FileChangeView[]; seen: TagEditorView | null; earlier: TagApplyReport[] }
        | { step: 'writing' }
        /**
         * This press's applies; the last one's would-drop files ask. `seen` is the view after it: an edit made while
         * the choice shows lists again.
         */
        | { step: 'wouldDrop'; reports: TagApplyReport[]; seen: TagEditorView | null; answering: boolean }
        | { step: 'done'; outcome: Outcome }
        /** The app refused before writing any file (it couldn't keep the old tags); the tab words why. */
        | { step: 'refused' }
        | { step: 'undone'; restored: number; left: number }

    let current = $state.raw<Step>({ step: 'idle' })
    let kind = $state<ContentKind>('audiobook')

    const view = $derived(tags.view)
    const canWrite = $derived(view !== null && view.editable)
    const KINDS: readonly ContentKind[] = ['audiobook', 'music']
    const kindOptions = $derived([
        { value: 'audiobook', label: tString('multiRename.tags.kind.audiobook') },
        { value: 'music', label: tString('multiRename.tags.kind.music') },
    ])
    const kindLabel = $derived(kindOptions.find((option) => option.value === kind)?.label ?? '')
    const fieldNames = (fields: readonly TagField[]): string[] =>
        fields.map((field) => tString(FIELD_LABELS[field]))

    function changeText(change: FileChangeView): string {
        const parts = fieldNames(change.fields)
        if (change.cover) parts.push(tString('multiRename.tags.write.cover'))
        if (change.extras) parts.push(tString('multiRename.tags.write.extras'))
        return tString('multiRename.tags.write.fileChanges', { fileName: change.fileName, fields: formatUnitList(parts) })
    }

    /** Lists what a write will change. `earlier` carries this press's applies so far through a re-list. */
    async function review(earlier: TagApplyReport[] = []): Promise<void> {
        await tags.settled()
        current = { step: 'checking' }
        const res = await tags.changes()
        // A failure is worded by the tab, from the state's error.
        if (res.ok) current = { step: 'review', changes: res.value, seen: tags.view, earlier }
        else current = endedStep(earlier)
    }

    /** Writes what `reviewed` listed. A list with nothing in it writes nothing (and so keeps the last Undo). */
    async function write(reviewed: Extract<Step, { step: 'review' }>): Promise<void> {
        const { changes, seen, earlier } = reviewed
        if (changes.length === 0) return
        await tags.settled()
        if (tags.view !== seen) {
            await review(earlier)
            return
        }
        current = { step: 'writing' }
        const res = await tags.apply({ kind, allowFrameLoss: false, skip: [] }, { after: earlier })
        if (!res.ok) {
            current = earlier.length > 0 ? endedStep(earlier) : refusedStep(res.error.kind)
            return
        }
        const reports = [...earlier, res.value]
        if (res.value.wouldDrop.length > 0) current = { step: 'wouldDrop', reports, seen: tags.view, answering: false }
        else current = endedStep(reports)
    }

    /** Where a press ends: what its applies did, or nothing to show when it made none. */
    function endedStep(reports: TagApplyReport[]): Step {
        return reports.length > 0 ? { step: 'done', outcome: outcomeOf(reports) } : { step: 'idle' }
    }

    /** Only these refusals come before any file is touched; another one may come after some were written. */
    function refusedStep(kind: string): Step {
        return kind === 'undo' || kind === 'noAppDataDir' ? { step: 'refused' } : { step: 'idle' }
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

    async function writeAnyway(reports: TagApplyReport[], seen: TagEditorView | null): Promise<void> {
        const last = reports.at(-1)
        if (last === undefined) return
        current = { step: 'wouldDrop', reports, seen, answering: true }
        await tags.settled()
        if (tags.view !== seen) {
            // Something was edited while the choice showed: nothing is written that wasn't listed. The applies so
            // far go along, so the next write continues them and their Undo isn't lost.
            await review(reports)
            return
        }
        const request = writeAnywayRequest(kind, last.wouldDrop, knownPaths(last))
        const res = await tags.apply(request, { after: reports })
        // A refused "Write anyway" leaves those files unwritten, as a skip does; the tab words the refusal.
        current = endedStep(res.ok ? [...reports, res.value] : reports)
    }

    /** The undo question: how many files go back, that later changes to them are overwritten, and unwritten edits too. */
    function undoQuestion(count: number): string {
        const message = tString('multiRename.tags.undo.message', { count, countText: formatInteger(count) })
        return tags.view?.dirty === true
            ? joinSentences([message, tString('multiRename.tags.undo.unwrittenLost')])
            : message
    }

    async function undo(): Promise<void> {
        await tags.settled()
        const last = tags.undoable
        if (last === null) return
        const sure = await confirmDialog(
            undoQuestion(undoableCount(last)),
            tString('multiRename.tags.undo.title'),
            tString('multiRename.tags.undo.confirm'),
        )
        if (!sure) return
        current = undoneStep(await tags.undo())
    }

    /** What an undo did: the files back as they were, and those still left (a failure leaves them undoable). */
    function undoneStep(res: TagUndoResult): Step {
        if (res.ok) {
            const left = res.value.missing.length + res.value.failed.length
            return { step: 'undone', restored: res.value.restored.length, left }
        }
        const left = tags.undoable === null ? 0 : undoableCount(tags.undoable)
        return { step: 'undone', restored: res.restored.restored.length, left }
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
        if (!canWrite) return
        if (current.step === 'review') await write(current)
        else if (['idle', 'done', 'refused', 'undone'].includes(current.step)) await review()
    }

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
                    kind: kindLabel,
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
            <Button variant="primary" disabled={!canWrite || changes.length === 0} keyHint={{ commandId: 'multiRename.rename' }} onclick={() => { void write(reviewed) }}>
                {tString('multiRename.tags.write.confirm')}
            </Button>
            <Button onclick={() => { current = endedStep(earlier) }}>{tString('multiRename.tags.write.back')}</Button>
        </div>
    {:else if current.step === 'wouldDrop'}
        {@const { reports, seen, answering } = current}
        <WouldDropDialog
            rows={reports.at(-1)?.wouldDrop ?? []}
            busy={answering}
            onWriteAnyway={() => { void writeAnyway(reports, seen) }}
            onSkip={() => { current = endedStep(reports) }}
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
        {:else if current.step === 'refused'}
            <p role="status">{tString('multiRename.tags.write.refused')}</p>
        {:else if current.step === 'undone'}
            {@const { restored, left } = current}
            <p role="status">
                {#if restored > 0}{tString('multiRename.tags.undo.done', { count: restored, countText: formatInteger(restored) })}{/if}
                {#if left > 0}{tString('multiRename.tags.undo.notRestored', { count: left, countText: formatInteger(left) })}{/if}
            </p>
        {/if}
        <div class="actions">
            <ToggleGroup
                semantics="toggles"
                value={kind}
                options={kindOptions}
                ariaLabel={tString('multiRename.tags.kind.label')}
                disabled={!canWrite}
                onChange={(value) => { kind = KINDS.find((k) => k === value) ?? kind }}
            />
            <Button
                variant="primary"
                disabled={!canWrite || view?.dirty !== true || current.step === 'checking' || current.step === 'writing'}
                keyHint={{ commandId: 'multiRename.rename' }}
                onclick={() => { void start() }}
            >
                {tString('multiRename.tags.write.review')}
            </Button>
        </div>
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
        flex-shrink: 0;
        max-height: 40%;
        overflow: auto;
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border);
    }

    p {
        margin: 0;
    }

    .warning {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        color: var(--color-warning-text);
    }

    .changes {
        margin: 0;
        padding-left: var(--spacing-lg);
        overflow-wrap: anywhere;
    }

    .actions {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        flex-wrap: wrap;
    }
</style>
