<script lang="ts">
    /**
     * Synchronize folders (⇧F12), Total Commander's Synchronize directories: the options strip, the show-filters,
     * the grid you review and override row by row, and the summary. It opens on the two panes' folders and
     * compares at once; Esc stops a running compare first, then closes. Keys: `sync-keys.ts`; what they do:
     * `sync-grid-controller.svelte.ts`; the backend session: `sync-session.svelte.ts`. `lib/sync/CLAUDE.md`.
     */
    import { onDestroy, onMount } from 'svelte'
    import { claimMenuCommand, createKeyRoadEcho, type KeyRoad } from '#lib/commands/menu-claims.js'
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
    import SyncConfirmDialog from './SyncConfirmDialog.svelte'
    import SyncRunPanel from './SyncRunPanel.svelte'
    import SyncProfilesControl from './SyncProfilesControl.svelte'
    import { createSyncRun } from './sync-run.svelte'
    import { createSyncProfiles, type LoadedProfile } from './profiles.svelte'
    import { showInGridFilters, versionsFolderPane, type ShowFolder } from './sync-run-view'
    import { createSyncSession } from './sync-session.svelte'
    import { SyncGridController } from './sync-grid-controller.svelte'
    import { syncKeyAction, type SyncKeyAction } from './sync-keys'
    import { syncIpc, readSyncDefaults } from './sync-ipc'
    import { syncProblemText } from './sync-errors'
    import { rowMenuSections } from './sync-row-menu'
    import type { SyncRoots } from './pane-roots'
    import type { SyncGridApi } from './sync-grid-api'
    import { SYNC_MENU_KEYS } from './sync-menu-claims'
    import type { KeepDestinationWire, ShowFilters } from '#lib/tauri-commands/sync.js'

    interface Props {
        roots: SyncRoots
        onClose: () => void
        /** Open with the Profiles menu up (the palette's "Synchronize folders: profiles…"). */
        openProfiles?: boolean
        /** "Show in Cmdr" on the result: open a versions folder in a pane (the dialog closes first). */
        onShowFolder?: (place: ShowFolder) => void
    }

    const { roots, onClose, openProfiles = false, onShowFolder }: Props = $props()

    // The roots are the panes' at open; Swap changes the session's own copy.
    const session = createSyncSession({
        ipc: syncIpc,
        roots,
        readDefaults: readSyncDefaults,
        includeHidden: getShowHiddenFiles,
    })

    const run = createSyncRun({ ipc: syncIpc, session, readDefaults: readSyncDefaults })
    const profiles = createSyncProfiles(syncIpc)
    let profilesControl: { toggleMenu: () => void; openSave: () => void } | undefined = $state()
    let loadedProfileId = $state<string | null>(null)

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
            confirm: () => {
                if (canSynchronize) void run.openConfirm()
                else controller.notice = tString('sync.dialog.synchronizeNeedsCompare')
            },
            openRowMenu,
            profiles: () => profilesControl?.toggleMenu(),
            saveProfile: () => profilesControl?.openSave(),
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
    /** The run (or its result) has the dialog: the grid's keys and menu claims rest. */
    const runShown = $derived(run.phase === 'running' || run.phase === 'paused' || run.phase === 'finished')

    /** One echo filter per action: the keyboard and menu halves of one press run it once. */
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping per press; nothing renders from it
    const echoes = new Map<string, (road: KeyRoad) => boolean>()

    function fire(action: SyncKeyAction, road: KeyRoad): void {
        const key = JSON.stringify(action)
        let isEcho = echoes.get(key)
        if (!isEcho) {
            isEcho = createKeyRoadEcho()
            echoes.set(key, isEcho)
        }
        if (isEcho(road)) return
        void controller.run(action)
    }

    onMount(() => {
        void session.open()
        void run.open()
        requestAnimationFrame(() => {
            if (openProfiles) profilesControl?.toggleMenu()
            else grid?.focus()
        })
        const releases = SYNC_MENU_KEYS.map(({ command, action, gridOnly }) =>
            claimMenuCommand(command, () => {
                // The confirm or the run is up: the key belongs to neither the grid nor the menu item.
                if (run.phase !== 'idle') return true
                if (gridOnly && !inGrid(document.activeElement)) return false
                fire(action, 'menu')
                return true
            }),
        )
        return () => {
            for (const release of releases) release()
            rowMenu.destroy()
        }
    })

    // A crash under the dialog's boundary unmounts it without a close: the session and its listeners go anyway.
    onDestroy(() => {
        run.close()
        void session.close()
    })

    /** Esc: a running compare stops first; the next Esc closes. */
    async function requestClose(): Promise<void> {
        if (run.phase === 'finished') {
            await run.dismissResult()
            return
        }
        if (runShown) {
            controller.notice = tString('sync.dialog.runEscape')
            return
        }
        if (comparing) {
            await session.stop()
            return
        }
        await closeNow()
    }

    async function closeNow(): Promise<void> {
        if (run.phase === 'running' || run.phase === 'paused') {
            controller.notice = tString('sync.dialog.runEscape')
            return
        }
        if (closing) return
        closing = true
        const saved = await session.close()
        if (!saved) addToast(tString('sync.dialog.optionsNotSaved'), { level: 'warn' })
        onClose()
    }

    function inGrid(target: EventTarget | null): boolean {
        return target instanceof Node && Boolean(gridAreaEl?.contains(target))
    }

    /** ⇧↑ ⇧↓ extend the grid's selection; a plain letter or digit jumps to a name (type-to-jump). */
    function gridOwnKey(event: KeyboardEvent): boolean {
        const plain = !event.metaKey && !event.altKey && !event.ctrlKey
        // eslint-disable-next-line cmdr/no-raw-key-match -- the whole combo is pinned: ⇧ alone (`plain` rules out ⌘ ⌥ ⌃)
        if (plain && event.shiftKey && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
            const next = grid?.cursorTarget(new KeyboardEvent('keydown', { key: event.key }), controller.cursor)
            if (next !== null && next !== undefined) void controller.extend(next)
            return true
        }
        if (plain && !event.isComposing && /^[\p{L}\p{N}._]$/u.test(event.key)) {
            void controller.typeToJump(event.key)
            return true
        }
        return false
    }

    function handleKeydown(event: KeyboardEvent): void {
        if (runShown) return
        const area = inGrid(event.target) ? 'grid' : 'options'
        if (area === 'grid' && gridOwnKey(event)) {
            event.preventDefault()
            return
        }
        const action = syncKeyAction(event, area)
        if (!action) return
        event.preventDefault()
        fire(action, 'keyboard')
    }

    function loadProfile(profile: LoadedProfile): void {
        loadedProfileId = profile.id
        void session.useProfile(profile)
    }

    /** "Show in grid": the list from before the run, every row visible, the rows that didn't run selected. */
    function showInGrid(items: number[]): void {
        void run.dismissResult(true)
        session.selected.clear()
        for (const item of items) session.selected.add(item)
        session.setFilters(showInGridFilters())
        controller.notice = tString('sync.result.notRunSelected', {
            count: items.length,
            countText: formatNumber(items.length),
        })
    }

    async function showFolder(folder: Extract<KeepDestinationWire, { type: 'folder' }>): Promise<void> {
        const place = { pane: versionsFolderPane(session.roots, folder.volumeId), volumeId: folder.volumeId, path: folder.path }
        await closeNow()
        onShowFolder?.(place)
    }

    const problem = $derived(session.error ?? (run.phase === 'idle' ? run.problem : null))

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
        {#if problem}
            <div class="problem" role="alert">
                <span>{syncProblemText(problem)}</span>
                {#if problem.type === 'volumeNotConnected'}
                    <Button size="mini" onclick={() => void session.usePaneFolder(problem.side)}
                        >{tString(
                            problem.side === 'left' ? 'sync.dialog.usePaneFolderLeft' : 'sync.dialog.usePaneFolderRight',
                        )}</Button
                    >
                {/if}
            </div>
        {/if}

        {#if runShown}
            <div class="grid-area">
                <SyncRunPanel {run} onShowFolder={(folder: Extract<KeepDestinationWire, { type: 'folder' }>) => void showFolder(folder)} onShowInGrid={showInGrid} />
            </div>
        {:else}
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
        {/if}

        <p class="status" aria-live="polite">{controller.notice ?? progressText}</p>
        <SyncSummaryBar
            state={session.state}
            onShow={(patch: Partial<ShowFilters>) => {
                session.setFilters({ ...session.filters, ...patch })
            }}
        />
    </div>

    {#snippet footerLeading()}
        <SyncProfilesControl
            bind:this={profilesControl}
            {profiles}
            loadedId={loadedProfileId}
            source={() => ({
                left: session.roots.left.root,
                right: session.roots.right.root,
                options: session.options,
                showFilters: session.filters,
                overrides: session.overrides,
            })}
            onLoad={loadProfile}
        />
    {/snippet}

    {#snippet footer()}
        {#if runShown}
            <!-- The run panel has its own buttons. -->
        {:else if comparing}
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
                onclick={() => void controller.run({ kind: 'confirm' })}>{tString('sync.dialog.synchronize')}</Button
            >
        {/if}
    {/snippet}
</ModalDialog>

<Menu menu={rowMenu} ariaLabel={tString('sync.menu.label')} minWidth={240} />

{#if run.phase === 'confirming'}<SyncConfirmDialog {run} />{/if}

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
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
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
