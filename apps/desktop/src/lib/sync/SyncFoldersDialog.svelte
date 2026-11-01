<script lang="ts">
    /**
     * Synchronize folders (⇧F12), Total Commander's Synchronize directories: the options strip, the show-filters,
     * the grid you review and override row by row, and the summary. It opens on the two panes' folders and
     * compares at once; Esc stops a running compare first, then closes. Keys: `sync-keys.ts`; what they do:
     * `sync-grid-controller.svelte.ts`; the backend session: `sync-session.svelte.ts`. `lib/sync/CLAUDE.md`.
     */
    import { onMount } from 'svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Menu from '#lib/ui/Menu.svelte'
    import { createMenu } from '#lib/ui/menu-controller.svelte.js'
    import { addToast } from '#lib/ui/toast/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { getShowHiddenFiles } from '#lib/settings/reactive-settings.svelte.js'
    import { openFileViewer } from '#lib/file-viewer/open-viewer.js'
    import { openInEditorOrExplain } from '#lib/file-explorer/pane/editor-open.js'
    import { copyToClipboard, quickLookOpen } from '#lib/tauri-commands/index.js'
    import SyncOptionsStrip from './SyncOptionsStrip.svelte'
    import SyncFilterBar from './SyncFilterBar.svelte'
    import SyncGrid from './SyncGrid.svelte'
    import SyncSummaryBar from './SyncSummaryBar.svelte'
    import { createSyncSession } from './sync-session.svelte'
    import { SyncGridController } from './sync-grid-controller.svelte'
    import { syncKeyAction, type SyncKeyAction } from './sync-keys'
    import { syncIpc, readSyncDefaults } from './sync-ipc'
    import { syncProblemText } from './sync-errors'
    import { rowMenuSections } from './sync-row-menu'
    import type { SyncRoots } from './pane-roots'
    import type { SyncGridApi } from './sync-grid-api'
    import type { ShowFilters } from '#lib/tauri-commands/index.js'

    interface Props {
        roots: SyncRoots
        onClose: () => void
    }

    const { roots, onClose }: Props = $props()

    // The roots are the panes' at open; Swap changes the session's own copy.
    const session = createSyncSession({
        ipc: syncIpc,
        roots,
        readDefaults: readSyncDefaults,
        includeHidden: getShowHiddenFiles,
    })

    let grid: SyncGridApi | undefined = $state()
    let gridAreaEl: HTMLDivElement | undefined = $state()
    let closing = false

    const controller = new SyncGridController({
        session,
        files: {
            view: (file) => openFileViewer(file.path, file.volumeId),
            edit: async (file) => {
                await openInEditorOrExplain(file.volumeId, file.path)
            },
            quickLook: (file) => quickLookOpen(file.path, file.volumeId),
            copyText: copyToClipboard,
        },
        reveal: (index) => grid?.scrollIndexIntoView(index),
        dialog: {
            close: () => {
                void requestClose()
            },
            // The confirm step (what runs, grouped, with the guards) arrives with the run; until then ⌘Enter has
            // nothing to open, and the Synchronize button says why.
            confirm: () => {},
            openRowMenu,
        },
    })

    const rowMenu = createMenu<SyncKeyAction>({
        getSections: () => rowMenuSections(controller.cursorRow),
        onSelect: (item) => {
            if (item.data) void controller.run(item.data)
        },
        restoreFocus: () => grid?.focus(),
    })

    function openRowMenu(at?: { x: number; y: number }): void {
        if (at) {
            rowMenu.openAt(at)
            return
        }
        const rowEl = gridAreaEl?.querySelector<HTMLElement>(`[data-index="${String(controller.cursor)}"]`)
        const box = (rowEl ?? gridAreaEl)?.getBoundingClientRect()
        if (box) rowMenu.openAt({ x: box.left + box.width / 2, y: box.bottom })
    }

    const comparing = $derived(session.phase === 'comparing' || session.phase === 'paused')
    const canSynchronize = $derived(session.phase === 'done' && !session.stale)

    onMount(() => {
        void session.open()
        requestAnimationFrame(() => grid?.focus())
        return () => {
            rowMenu.destroy()
        }
    })

    /** Esc: a running compare stops first; the next Esc closes. */
    async function requestClose(): Promise<void> {
        if (comparing) {
            await session.stop()
            return
        }
        await closeNow()
    }

    async function closeNow(): Promise<void> {
        if (closing) return
        closing = true
        const refusal = await session.close()
        if (refusal) addToast(tString('sync.dialog.optionsNotSaved'), { level: 'warn' })
        onClose()
    }

    function inGrid(target: EventTarget | null): boolean {
        return target instanceof Node && Boolean(gridAreaEl?.contains(target))
    }

    /** ⇧↑ ⇧↓ and ⌘A belong to the grid's selection, before the key map. */
    function selectionKey(event: KeyboardEvent): boolean {
        const plainShift = event.shiftKey && !event.metaKey && !event.altKey && !event.ctrlKey
        if (plainShift && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
            const next = grid?.cursorTarget(new KeyboardEvent('keydown', { key: event.key }), controller.cursor)
            if (next !== null && next !== undefined) void controller.extend(next)
            return true
        }
        if (event.metaKey && !event.shiftKey && !event.altKey && !event.ctrlKey && event.code === 'KeyA') {
            void controller.selectAll()
            return true
        }
        return false
    }

    function handleKeydown(event: KeyboardEvent): void {
        const area = inGrid(event.target) ? 'grid' : 'options'
        if (area === 'grid' && selectionKey(event)) {
            event.preventDefault()
            return
        }
        const action = syncKeyAction(event, area)
        if (!action) return
        // Enter on a button presses it (ModalDialog already did); only a field's Enter compares.
        if (action.kind === 'compare' && event.key === 'Enter' && event.target instanceof HTMLButtonElement) return
        event.preventDefault()
        void controller.run(action)
    }

    const progressText = $derived.by(() => {
        const progress = session.progress
        if (session.phase === 'paused') return tString('sync.dialog.paused')
        if (session.phase === 'stopped') return tString('sync.dialog.stopped')
        if (!comparing || !progress) return ''
        if (progress.content) {
            const { filesDone, filesTotal } = progress.content
            return tString('sync.dialog.comparingContent', {
                doneText: formatNumber(filesDone),
                total: filesTotal,
                totalText: formatNumber(filesTotal),
            })
        }
        return tString('sync.dialog.comparing', {
            leftFiles: progress.left.files,
            leftFilesText: formatNumber(progress.left.files),
            rightFiles: progress.right.files,
            rightFilesText: formatNumber(progress.right.files),
        })
    })

    const emptyText = $derived.by(() => {
        if (session.phase !== 'done' || session.total > 0 || !session.state) return ''
        const s = session.state.summary
        const differs = s.different + s.uniqueLeft + s.uniqueRight + s.undecided + s.notComparable
        return tString(differs === 0 ? 'sync.grid.identical' : 'sync.grid.empty')
    })
</script>

<ModalDialog
    titleId="sync-folders-title"
    dialogId="sync-folders"
    resizable
    fillBody
    containerStyle="width: min(1200px, 94vw); min-width: 720px; height: min(820px, 90vh)"
    onclose={() => void closeNow()}
    onescape={() => void requestClose()}
    onkeydown={handleKeydown}
>
    {#snippet title()}{tString('sync.dialog.title')}{/snippet}

    <div class="sync-dialog">
        <SyncOptionsStrip {session} onSwap={() => void session.swap()} />
        <SyncFilterBar filters={session.filters} onToggle={(key: keyof ShowFilters) => {
                controller.toggleFilter(key)
            }} />

        {#if session.stale && !comparing}
            <div class="banner" role="status">
                <span>{tString(session.state?.stale ? 'sync.dialog.staleFolders' : 'sync.dialog.stale')}</span>
                <Button size="mini" variant="secondary" onclick={() => void session.compare()}
                    >{tString('sync.dialog.compareAgain')}</Button
                >
            </div>
        {/if}
        {#if session.error}
            <p class="problem" role="alert">{syncProblemText(session.error)}</p>
        {/if}

        <div class="grid-area" bind:this={gridAreaEl}>
            <SyncGrid
                bind:this={grid}
                {session}
                cursorIndex={controller.cursor}
                onCursorChange={(index: number) => {
                    controller.moveCursor(index)
                }}
                onKeydown={() => {}}
                onRowClick={(index: number, event: MouseEvent) => void controller.click(index, event)}
                onRowDoubleClick={() => void controller.run({ kind: 'compareContent' })}
                onRowContextMenu={({ index, event }: { index: number; event: MouseEvent }) => {
                    controller.moveCursor(index)
                    openRowMenu({ x: event.clientX, y: event.clientY })
                }}
            />
            {#if emptyText}<p class="empty">{emptyText}</p>{/if}
        </div>

        <p class="status" aria-live="polite">{controller.notice ?? progressText}</p>
        <SyncSummaryBar
            state={session.state}
            onShow={(patch: Partial<ShowFilters>) => {
                session.setFilters({ ...session.filters, ...patch })
            }}
        />
    </div>

    {#snippet footer()}
        {#if comparing}
            <Button variant="secondary" onclick={() => void session.stop()}>{tString('sync.dialog.stop')}</Button>
            <Button
                variant="primary"
                onclick={() => void session.setPaused(session.phase !== 'paused')}
                >{tString(session.phase === 'paused' ? 'sync.dialog.resume' : 'sync.dialog.pause')}</Button
            >
        {:else}
            <Button variant="secondary" onclick={() => void closeNow()}>{tString('sync.dialog.close')}</Button>
            <Button variant="secondary" keyHint="⌥⌘R" onclick={() => void session.compare()}
                >{tString('sync.dialog.compare')}</Button
            >
            <Button
                variant="primary"
                ariaDisabled={!canSynchronize}
                tooltipContent={tString('sync.dialog.synchronizeNeedsCompare')}
                keyHint="⌘↩"
                onclick={() => {
                    if (canSynchronize) void controller.run({ kind: 'confirm' })
                    else controller.notice = tString('sync.dialog.synchronizeNeedsCompare')
                }}>{tString('sync.dialog.synchronize')}</Button
            >
        {/if}
    {/snippet}
</ModalDialog>

<Menu menu={rowMenu} ariaLabel={tString('sync.menu.label')} minWidth={240} />

<style>
    .sync-dialog {
        display: flex;
        flex: 1 1 auto;
        flex-direction: column;
        gap: var(--spacing-sm);
        min-height: 0;
    }

    .grid-area {
        position: relative;
        display: flex;
        flex: 1 1 auto;
        min-height: 200px;
    }

    .empty {
        position: absolute;
        inset: 40% 0 auto;
        margin: 0;
        color: var(--color-text-secondary);
        text-align: center;
    }

    .banner {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-warning-bg);
    }

    .problem {
        margin: 0;
        color: var(--color-error-text);
    }

    .status {
        min-height: 1lh;
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }
</style>
