<script lang="ts">
    /**
     * The run in the Synchronize folders dialog: one aggregate bar (items, bytes, ETA) with Pause and Cancel, which
     * take effect between steps (said, not hidden), then the result: every way a row ended by its typed reason, the
     * versions folder with "Show in Cmdr", "Show in grid" for the rows that didn't run, and "Compare again" (a run
     * leaves the compare stale). Each copy and move also shows in the transfer queue, as any operation does.
     */
    import Button from '#lib/ui/Button.svelte'
    import ProgressBar from '#lib/ui/ProgressBar.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { formatByteSize, formatDuration, seconds } from '#lib/units/index.js'
    import type { KeepDestinationWire } from '#lib/tauri-commands/sync.js'
    import { resultLines, type ResultKind } from './run-result'
    import { syncProblemText } from './sync-errors'
    import type { SyncRun } from './sync-run.svelte'

    type VersionsFolder = Extract<KeepDestinationWire, { type: 'folder' }>

    interface Props {
        run: SyncRun
        /** "Show in Cmdr": open a versions folder in a pane. */
        onShowFolder: (folder: VersionsFolder) => void
        /** "Show in grid": back to the list with the rows that didn't run selected. */
        onShowInGrid: (items: number[]) => void
    }

    const { run, onShowFolder, onShowInGrid }: Props = $props()

    const RESULT_KEY = {
        done: 'sync.result.done',
        changedSinceCompare: 'sync.result.changedSinceCompare',
        noTrash: 'sync.result.noTrash',
        couldNotDo: 'sync.result.couldNotDo',
        versionsSpotTaken: 'sync.result.versionsSpotTaken',
        versionsFolderUnsafe: 'sync.result.versionsFolderUnsafe',
        keptNotReplaced: 'sync.result.keptNotReplaced',
        heldAfterFailedCopy: 'sync.result.heldAfterFailedCopy',
        heldByGuard: 'sync.result.heldByGuard',
        notStarted: 'sync.result.notStarted',
    } as const satisfies Record<ResultKind, MessageKey>

    const progress = $derived(run.progress)
    const progressText = $derived(
        progress
            ? tString('sync.run.progress', {
                  doneText: formatNumber(progress.itemsDone),
                  totalText: formatNumber(progress.itemsTotal),
                  total: progress.itemsTotal,
                  bytesDone: formatByteSize(progress.bytesDone),
                  bytesTotal: formatByteSize(progress.bytesTotal),
              })
            : '',
    )
    const fraction = $derived.by(() => {
        if (!progress) return 0
        if (progress.bytesTotal > 0) return progress.bytesDone / progress.bytesTotal
        return progress.itemsTotal > 0 ? progress.itemsDone / progress.itemsTotal : 0
    })
    const eta = $derived(run.etaSeconds)

    const report = $derived(run.report)
    const folders = $derived(
        (report?.versionsFolders ?? []).filter((folder): folder is VersionsFolder => folder.type === 'folder'),
    )
    const notDone = $derived(run.notDone)
    /** Folders the run made and leaves: they stay, since a run never removes anything it can't keep. */
    const leftBehind = $derived([
        ...(report?.emptyFolders ?? []).map((folder) => tString('sync.result.emptyFolder', { path: folder.path })),
        ...(report?.leftoverFolders ?? []).map((folder) => tString('sync.result.leftoverFolder', { path: folder.path })),
    ])
</script>

{#if run.phase === 'running' || run.phase === 'paused'}
    <section class="run" aria-labelledby="sync-run-title">
        <h3 id="sync-run-title">{tString('sync.run.title')}</h3>
        <ProgressBar
            value={fraction}
            animated={run.phase === 'running'}
            ariaLabel={tString('sync.run.progressLabel')}
            ariaValuetext={progressText}
        />
        <p aria-live="polite">{progressText}</p>
        {#if eta !== null}<p>{tString('sync.run.eta', { duration: formatDuration(seconds(eta)) })}</p>{/if}
        <p class="note">
            {tString(run.pausing ? 'sync.run.pausing' : run.phase === 'paused' ? 'sync.run.paused' : 'sync.run.betweenSteps')}
        </p>
        {#if run.problem}<p class="problem" role="alert">{syncProblemText(run.problem)}</p>{/if}
        <div class="actions">
            <Button variant="secondary" disabled={run.cancelling} onclick={() => void run.cancel()}
                >{tString(run.cancelling ? 'sync.run.stopping' : 'sync.run.cancel')}</Button
            >
            <Button variant="primary" disabled={run.cancelling} onclick={() => void run.setPaused(run.phase !== 'paused')}
                >{tString(run.phase === 'paused' ? 'sync.run.resume' : 'sync.run.pause')}</Button
            >
        </div>
    </section>
{:else if run.phase === 'finished' && report}
    <section class="run" aria-labelledby="sync-result-title">
        <h3 id="sync-result-title">{tString(report.stopped ? 'sync.result.titleStopped' : 'sync.result.title')}</h3>
        <div role="status">
            <ul>
                {#each resultLines(report) as line (line.kind)}
                    <li>{tString(RESULT_KEY[line.kind], { count: line.count, countText: formatNumber(line.count) })}</li>
                {/each}
            </ul>
        </div>
        {#each folders as folder (folder.path)}
            <div class="folder">
                <span
                    >{folders.length === 1
                        ? tString('sync.result.versionsFolder', { size: formatByteSize(report.keptBytes), path: folder.path })
                        : tString('sync.result.versionsFolderShort', { path: folder.path })}</span
                >
                <Button size="mini" onclick={() => { onShowFolder(folder) }}>{tString('sync.result.showInCmdr')}</Button>
            </div>
        {/each}
        {#if leftBehind.length > 0}
            <div class="left-behind">
                <p>{tString('sync.result.foldersStay')}</p>
                <ul>
                    {#each leftBehind as line (line)}<li>{line}</li>{/each}
                </ul>
            </div>
        {/if}
        <div class="actions">
            {#if notDone.length > 0}
                <Button variant="secondary" onclick={() => { onShowInGrid(notDone) }}>{tString('sync.result.showInGrid')}</Button>
            {/if}
            <Button variant="primary" autoFocus onclick={() => void run.dismissResult()}
                >{tString('sync.result.compareAgain')}</Button
            >
        </div>
    </section>
{/if}

<style>
    .run {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: min(560px, 100%);
        margin: auto;
        padding: var(--spacing-lg);
    }

    .run h3,
    .run p,
    .run ul {
        margin: 0;
    }

    .run ul {
        padding-left: var(--spacing-lg);
    }

    .left-behind {
        overflow-wrap: anywhere;
    }

    .note {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .problem {
        color: var(--color-error-text);
    }

    .folder {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        overflow-wrap: anywhere;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
    }
</style>
