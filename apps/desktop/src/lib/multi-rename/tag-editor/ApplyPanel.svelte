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
    import type { ContentKind, TagField } from '#lib/ipc/bindings.js'
    import type { FileChangeView, RestoreReport, TagApplyReport, TagEditorView } from '#lib/tauri-commands/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToggleGroup from '#lib/ui/ToggleGroup.svelte'
    import { confirmDialog } from '#lib/utils/confirm-dialog.js'
    import ApplyOutcome from './ApplyOutcome.svelte'
    import WouldDropDialog from './WouldDropDialog.svelte'
    import { outcomeOf, writeAnywayRequest, type ApplyOutcome as Outcome } from './apply-outcome'
    import { FIELD_LABELS } from './field-labels'
    import type { TagEditorState } from './tag-editor-state.svelte'

    interface Props {
        tags: TagEditorState
    }

    const { tags }: Props = $props()

    type Step =
        | { step: 'idle' }
        | { step: 'checking' }
        /** The list shown, and the view it was made from: an edit since then lists again before any write. */
        | { step: 'review'; changes: FileChangeView[]; seen: TagEditorView | null }
        | { step: 'writing' }
        | { step: 'wouldDrop'; first: TagApplyReport; answering: boolean }
        | { step: 'done'; outcome: Outcome }
        | { step: 'undone'; report: RestoreReport }

    let current = $state.raw<Step>({ step: 'idle' })
    let kind = $state<ContentKind>('audiobook')

    const view = $derived(tags.view)
    const canWrite = $derived(view !== null && view.editable)
    const KINDS: readonly ContentKind[] = ['audiobook', 'music']
    const kindOptions = $derived([
        { value: 'audiobook', label: tString('multiRename.tags.kind.audiobook') },
        { value: 'music', label: tString('multiRename.tags.kind.music') },
    ])
    const fieldNames = (fields: readonly TagField[]): string[] =>
        fields.map((field) => tString(FIELD_LABELS[field]))

    function changeText(change: FileChangeView): string {
        const parts = fieldNames(change.fields)
        if (change.cover) parts.push(tString('multiRename.tags.write.cover'))
        if (change.extras) parts.push(tString('multiRename.tags.write.extras'))
        return tString('multiRename.tags.write.fileChanges', { fileName: change.fileName, fields: formatUnitList(parts) })
    }

    async function review(): Promise<void> {
        await tags.settled()
        current = { step: 'checking' }
        const res = await tags.changes()
        // A failure is worded by the tab, from the state's error.
        current = res.ok ? { step: 'review', changes: res.value, seen: tags.view } : { step: 'idle' }
    }

    async function write(seen: TagEditorView | null): Promise<void> {
        await tags.settled()
        if (tags.view !== seen) {
            await review()
            return
        }
        current = { step: 'writing' }
        const res = await tags.apply({ kind, allowFrameLoss: false, skip: [] })
        if (!res.ok) current = { step: 'idle' }
        else if (res.value.wouldDrop.length > 0) current = { step: 'wouldDrop', first: res.value, answering: false }
        else current = { step: 'done', outcome: outcomeOf(res.value) }
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

    async function writeAnyway(first: TagApplyReport): Promise<void> {
        current = { step: 'wouldDrop', first, answering: true }
        const request = writeAnywayRequest(kind, first.wouldDrop, knownPaths(first))
        const res = await tags.apply(request, { continuing: true })
        // A refused second write leaves those files unwritten, as a skip does; the tab words the refusal.
        current = { step: 'done', outcome: res.ok ? outcomeOf(first, { second: res.value }) : outcomeOf(first, { skipped: true }) }
    }

    function skip(first: TagApplyReport): void {
        current = { step: 'done', outcome: outcomeOf(first, { skipped: true }) }
    }

    async function undo(): Promise<void> {
        const last = tags.undoable
        if (last === null) return
        const params = { count: last.written, countText: formatInteger(last.written) }
        const sure = await confirmDialog(
            tString('multiRename.tags.undo.message', params),
            tString('multiRename.tags.undo.title'),
            tString('multiRename.tags.undo.confirm'),
        )
        if (!sure) return
        const res = await tags.undo()
        if (res.ok) current = { step: 'undone', report: res.value }
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
        if (current.step === 'review') await write(current.seen)
        else if (current.step === 'idle' || current.step === 'done' || current.step === 'undone') await review()
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
                onclick={() => { void tags.tool({ kind: 'stripDiacritics', fields: [...view.diacritics] }) }}
            >
                {tString('multiRename.tags.diacritics.fix')}
            </Button>
        </p>
    {/if}

    {#if current.step === 'review'}
        {@const { changes, seen } = current}
        {#if changes.length === 0}
            <p>{tString('multiRename.tags.write.nothing')}</p>
        {:else}
            <p>{tString('multiRename.tags.write.summary', { count: changes.length, countText: formatInteger(changes.length) })}</p>
            <ul class="changes" aria-label={tString('multiRename.tags.write.changeLabel')}>
                {#each changes as change (change.path)}
                    <li>{changeText(change)}</li>
                {/each}
            </ul>
        {/if}
        <div class="actions">
            <Button variant="primary" disabled={!canWrite || changes.length === 0} keyHint={{ commandId: 'multiRename.rename' }} onclick={() => { void write(seen) }}>
                {tString('multiRename.tags.write.confirm')}
            </Button>
            <Button onclick={() => { current = { step: 'idle' } }}>{tString('multiRename.tags.write.back')}</Button>
        </div>
    {:else if current.step === 'wouldDrop'}
        {@const { first, answering } = current}
        <WouldDropDialog
            rows={first.wouldDrop}
            busy={answering}
            onWriteAnyway={() => { void writeAnyway(first) }}
            onSkip={() => { skip(first) }}
        />
    {:else}
        {#if current.step === 'checking'}
            <p role="status">{tString('multiRename.tags.write.checking')}</p>
        {:else if current.step === 'writing'}
            <p role="status">{tString('multiRename.tags.write.writing')}</p>
        {:else if current.step === 'done'}
            <ApplyOutcome outcome={current.outcome} onReload={() => { void reload() }} />
        {:else if current.step === 'undone'}
            {@const { report } = current}
            {@const left = report.missing.length + report.failed.length}
            <p role="status">
                {tString('multiRename.tags.undo.done', { count: report.restored.length, countText: formatInteger(report.restored.length) })}
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
