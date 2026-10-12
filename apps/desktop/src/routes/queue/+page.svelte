<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import { initWindowSettings, initWindowLanguageSync } from '#lib/settings/window-settings.js'
    import { initAccentColor, cleanupAccentColor } from '#lib/accent-color.js'
    import { initGlassMaterial, cleanupGlassMaterial } from '#lib/glass-material.js'
    import { initTextSize, cleanupTextSize } from '#lib/text-size.svelte.js'
    import { trackOwnRect } from '#lib/window-positioning.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import {
        cancelOperations,
        dismissAllFailedOperations,
        dismissFailedOperation,
        moveQueuedOperation,
        onQueueSelectAll,
        pauseAll,
        resumeAll,
        type QueueMoveDirection,
    } from '#lib/tauri-commands/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import QueueRow from '#lib/file-operations/queue/QueueRow.svelte'
    import { createQueueCursor } from '#lib/file-operations/queue/queue-cursor.svelte.js'
    import { queueKeyAction, type QueueKeyAction, type QueueRowApi } from '#lib/file-operations/queue/queue-keys.js'
    import {
        createOperationsStore,
        isHiddenSettledStatus,
        isTerminalStatus,
    } from '#lib/file-operations/queue/operations-store.svelte.js'
    import { treesCollapsible } from '#lib/file-operations/queue/row-tree.js'
    import {
        initOperationSessions,
        destroyOperationSessions,
    } from '#lib/file-operations/operation-session/window-operation-sessions.svelte.js'

    const log = getAppLogger('queue')

    const store = createOperationsStore()

    let initialized = $state(false)
    let unlistenFocusSelf: UnlistenFn | undefined
    let unlistenRectTracking: (() => void) | undefined
    let unsubscribeLanguage: (() => void) | undefined
    let unlistenSelectAll: UnlistenFn | undefined

    /** Ids the user has checked for "Cancel selected". A `SvelteSet` so toggling
     *  is O(1) AND reactive on in-place mutation (add/delete), per the project's
     *  selection pattern. */
    const selectedIds = new SvelteSet<string>()

    /** Live work, plus every retained failure. A `done` or `cancelled` op can
     *  linger in the snapshot for a tick before the backend prunes it, and it
     *  has nothing left to say, so it stays hidden; a failure stays visible
     *  until someone dismisses it. */
    const rows = $derived(store.operations.filter((row) => !isHiddenSettledStatus(row.snapshot.status)))
    const isEmpty = $derived(rows.length === 0)
    /** Two or more rows show their live tree, so each gets a collapse toggle. */
    const treeCollapsible = $derived(treesCollapsible(rows))

    // Selection that no longer points at something cancelable gets dropped so the
    // count and "Cancel selected" stay honest. Membership alone isn't the test: a
    // failure STAYS in `rows` and drops its checkbox, so a tick the user made
    // while it was running would have no way back out, and the toolbar would
    // offer to cancel an operation that has already stopped.
    $effect(() => {
        const cancelableIds = new Set(
            rows.filter((r) => !isTerminalStatus(r.snapshot.status)).map((r) => r.snapshot.operationId),
        )
        for (const id of selectedIds) {
            if (!cancelableIds.has(id)) selectedIds.delete(id)
        }
    })

    const selectedCount = $derived(selectedIds.size)

    // ── Keyboard (DETAILS § Keyboard) ───────────────────────────────────────
    // The list is an ARIA grid with ONE Tab stop: the cursor row. The arrows move
    // the cursor, ← / → reach the focused row's buttons, and the row's own keys
    // (Enter, ⌫) do what its buttons do. The cursor follows an operation id, so
    // a reorder or a finished operation never slides another one under it.

    const rowIds = $derived(rows.map((r) => r.snapshot.operationId))

    const cursor = createQueueCursor({
        ids: () => rowIds,
        selection: selectedIds,
        isSelectable: (id) => {
            const row = rows.find((r) => r.snapshot.operationId === id)
            return row !== undefined && !isTerminalStatus(row.snapshot.status)
        },
    })

    /** Each row's keyboard half (`QueueRow`'s exports), by operation id. */
    const rowApis: Record<string, QueueRowApi | undefined> = {}

    let gridEl: HTMLDivElement | undefined = $state()

    /** The grid held focus last, so a row that leaves (it finished, or a reorder
     *  moved its element) hands focus to the cursor's new row instead of dropping
     *  it on the page. Cleared only when focus genuinely moves elsewhere. */
    let gridOwnsFocus = $state(false)

    /** The live region's text: where a reordered operation landed. */
    let announcement = $state('')

    // The cursor follows its operation, or takes the row that slid into its place.
    // `reconcile` reads the ids, which is what re-runs this on every change.
    $effect(() => {
        cursor.reconcile()
    })

    // Keep focus on the cursor row while the grid owns it: a row that vanished, or
    // a keyed `{#each}` moving the focused element, leaves focus on <body>.
    // Reading `rowIds` here is what re-runs it after a reorder, when the cursor id stays put.
    $effect(() => {
        const id = cursor.cursorId
        if (!gridOwnsFocus || id === null || !gridEl || !rowIds.includes(id)) return
        const active = document.activeElement
        if (active === null || active === document.body || !gridEl.contains(active)) rowApis[id]?.focusRow()
    })

    /** The row Tab lands on: the cursor row, or the first row before there is one. */
    const tabStopId = $derived(
        cursor.cursorId !== null && rowIds.includes(cursor.cursorId) ? cursor.cursorId : (rowIds[0] ?? null),
    )

    function handleGridFocusOut(event: FocusEvent): void {
        const next = event.relatedTarget
        // `null` is focus going nowhere: a removed element, or the window losing
        // focus. Either way the grid still owns it when it comes back.
        if (next instanceof Node && !gridEl?.contains(next)) gridOwnsFocus = false
    }

    function focusCursorRow(): void {
        const id = cursor.cursorId
        if (id !== null) rowApis[id]?.focusRow()
    }

    async function moveCursorRow(direction: QueueMoveDirection): Promise<void> {
        const id = cursor.cursorId
        if (id === null) return
        try {
            const outcome = await moveQueuedOperation(id, direction)
            // Announced fresh each time: a live region repeats nothing it already says.
            announcement = ''
            await tick()
            announcement =
                outcome.kind === 'moved'
                    ? tString('queue.reorder.moved', { direction, position: outcome.position, waiting: outcome.waiting })
                    : outcome.kind === 'at_edge'
                      ? tString('queue.reorder.atEdge', { direction })
                      : tString('queue.reorder.notQueued')
        } catch (error) {
            log.warn('Failed to move operation {operationId}: {error}', { operationId: id, error: String(error) })
        }
    }

    /** Runs a key the grid answers. `false` leaves the key to whoever else wants it. */
    function runGridAction(action: QueueKeyAction): boolean {
        switch (action.kind) {
            case 'cursor':
                cursor.move(action.to)
                focusCursorRow()
                return true
            case 'extend':
                cursor.extend(action.to)
                focusCursorRow()
                return true
            case 'toggleSelected':
                cursor.toggle()
                return true
            case 'selectAll':
                cursor.selectAll()
                return true
            case 'move':
                void moveCursorRow(action.direction)
                return true
            case 'show':
            case 'cancelOrDismiss':
            case 'control':
                return runRowAction(action)
            case 'close':
                return false
        }
    }

    /** The keys that act on the cursor row's own operation, through the row. */
    function runRowAction(action: Extract<QueueKeyAction, { kind: 'show' | 'cancelOrDismiss' | 'control' }>): boolean {
        const id = cursor.cursorId
        const api = id === null ? undefined : rowApis[id]
        if (!api) return false
        if (action.kind === 'show') return api.show()
        if (action.kind === 'cancelOrDismiss') return api.cancelOrDismiss()
        return api.focusControl(action.step)
    }

    /** Whether the grid answers a key pressed on `target`. Inside the grid, yes;
     *  on the bare page too (nothing focused yet, or focus dropped after a click),
     *  so ↓ works the moment the window opens. ❌ Never inside a dialog (the
     *  rollback question has its own keys), on the toolbar, or inside a row's
     *  live tree: that's its own widget, where ⌫ cancelling the whole copy would
     *  be a trap. Escape there hands focus back to the row (`QueueRow`). */
    function gridHears(target: EventTarget | null): boolean {
        if (!(target instanceof Element)) return true
        if (target.closest('[role="dialog"], [role="alertdialog"], [data-queue-tree]')) return false
        if (gridEl?.contains(target)) return true
        return target === document.body || target.classList.contains('queue-window')
    }

    /** Enter and Space press a focused BUTTON natively, and ⌫ belongs to the
     *  row wherever focus is in it; the row's own keys apply only on the row. */
    function isRowOnlyKey(action: QueueKeyAction, target: EventTarget | null): boolean {
        return (action.kind === 'show' || action.kind === 'toggleSelected') && target instanceof HTMLButtonElement
    }

    function toggleSelect(operationId: string): void {
        if (selectedIds.has(operationId)) selectedIds.delete(operationId)
        else selectedIds.add(operationId)
    }

    // ❌ No per-row pause/resume/cancel/rollback here. A row commands its own
    // operation through the session it's already bound to, so the same press
    // means the same thing from every surface and no two of them can send it
    // twice. This page keeps only what ISN'T a command on one operation: the
    // fleet actions below and dismissing a retained failure.

    async function cancelSelected(): Promise<void> {
        const ids = [...selectedIds]
        if (ids.length === 0) return
        try {
            await cancelOperations(ids)
            selectedIds.clear()
        } catch (error) {
            log.warn('Failed to cancel selected operations: {error}', { error: String(error) })
        }
    }

    /** Stop showing one retained failure. Explicit only: nothing else in the app
     *  drops a failed row, so a copy that died while the user was away is still
     *  here when they come back. */
    async function dismissRow(operationId: string): Promise<void> {
        try {
            await dismissFailedOperation(operationId)
        } catch (error) {
            log.warn('Failed to dismiss operation {operationId}: {error}', { operationId, error: String(error) })
        }
    }

    async function dismissAll(): Promise<void> {
        try {
            await dismissAllFailedOperations()
        } catch (error) {
            log.warn('Failed to dismiss all failed operations: {error}', { error: String(error) })
        }
    }

    async function handlePauseAll(): Promise<void> {
        try {
            await pauseAll()
        } catch (error) {
            log.warn('Failed to pause all operations: {error}', { error: String(error) })
        }
    }

    async function handleResumeAll(): Promise<void> {
        try {
            await resumeAll()
        } catch (error) {
            log.warn('Failed to resume all operations: {error}', { error: String(error) })
        }
    }

    function closeWindow(): void {
        // Defer the close past the current event-loop tick so any in-flight IPC
        // ack settles before the webview is destroyed. `setTimeout(0)`, not rAF
        // (throttled when unfocused on macOS). Mirrors Settings / Shortcuts.
        const win = getCurrentWindow()
        setTimeout(() => {
            void win.close()
        }, 0)
    }

    function handleKeydown(event: KeyboardEvent): void {
        if (event.defaultPrevented) return
        const action = queueKeyAction(event)
        if (action === null) return
        if (action.kind === 'close') {
            event.preventDefault()
            closeWindow()
            return
        }
        if (!gridHears(event.target) || isRowOnlyKey(action, event.target)) return
        if (runGridAction(action)) event.preventDefault()
    }

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) loadingScreen.style.display = 'none'

        try {
            // Seeds the store AND the reactive layer that `<Size>` and friends read.
            // `window-settings.ts` knows this window has no store capability (see
            // `src-tauri/capabilities/CLAUDE.md` § queue — no persistence in v1), so it
            // takes the restricted path: the backend snapshot plus cross-window change
            // events, mirroring the viewer, and non-throwing so a perm failure can't
            // leave the body unrendered.
            await initWindowSettings()
            unsubscribeLanguage = initWindowLanguageSync()
            await initAccentColor()
            await initGlassMaterial()
            await initTextSize()
            await store.init()
            // This window's session registry. Subscribed here, before any row
            // can ask for a session: the fan-out's listeners are async, and
            // events that arrive while they're being set up would be lost.
            await initOperationSessions()
            initialized = true

            // Already-open window self-focuses on a re-open (cross-window setFocus()
            // doesn't reliably raise a window on macOS).
            unlistenFocusSelf = await listen('focus-self', () => {
                setTimeout(() => {
                    void getCurrentWindow().setFocus()
                }, 0)
            })

            // Remember position/size within the session so a reopen lands in place.
            unlistenRectTracking = await trackOwnRect('queue')

            // ⌘A reaches this window as a menu event, never as a keydown (the
            // accelerator fires first). The keydown road above stays for a menu
            // that doesn't claim it (the dev browser).
            unlistenSelectAll = await onQueueSelectAll(() => {
                cursor.selectAll()
            })
        } catch (error) {
            log.error('Failed to initialize operation-queue window: {error}', { error })
        }
    })

    onDestroy(() => {
        unlistenFocusSelf?.()
        unlistenRectTracking?.()
        unlistenSelectAll?.()
        unsubscribeLanguage?.()
        store.dispose()
        destroyOperationSessions()
        cleanupAccentColor()
        cleanupGlassMaterial()
        cleanupTextSize()
    })
</script>

<svelte:window onkeydown={handleKeydown} />

<main class="queue-window" tabindex="-1">
    <h1 class="sr-only">{tString('queue.windowTitle')}</h1>
    <!-- Drag strip under the overlay traffic lights, like Settings/Shortcuts. -->
    <div class="window-drag-region" data-tauri-drag-region aria-hidden="true"></div>

    <header class="queue-header">
        <div class="title-row">
            <span class="title">{tString('queue.heading')}</span>
        </div>
        <div class="toolbar" role="toolbar" aria-label={tString('queue.heading')}>
            <Button variant="secondary" size="mini" onclick={handlePauseAll} disabled={!store.hasRunning}>
                <span class="btn-inner"><Icon name="pause" size={13} />{tString('queue.toolbar.pauseAll')}</span>
            </Button>
            <Button variant="secondary" size="mini" onclick={handleResumeAll} disabled={!store.hasPaused}>
                <span class="btn-inner"><Icon name="play" size={13} />{tString('queue.toolbar.resumeAll')}</span>
            </Button>
            <!-- One failure has its own Dismiss on the row; the toolbar button
                 earns its place only once there's a pile to clear. -->
            {#if store.failureCount > 1}
                <Button variant="secondary" size="mini" onclick={() => void dismissAll()}>
                    <span class="btn-inner"><Icon name="x" size={13} />{tString('queue.toolbar.dismissAll')}</span>
                </Button>
            {/if}
            <span class="toolbar-spacer"></span>
            {#if selectedCount > 0}
                <span class="selected-count">{tString('queue.toolbar.selectedCount', { count: selectedCount })}</span>
            {/if}
            <Button variant="secondary" size="mini" onclick={cancelSelected} disabled={selectedCount === 0}>
                <span class="btn-inner"><Icon name="x" size={13} />{tString('queue.toolbar.cancelSelected')}</span>
            </Button>
        </div>
    </header>

    {#if initialized}
        {#if isEmpty}
            <div class="empty-state">
                <span class="empty-icon" aria-hidden="true"><Icon name="hourglass" size={28} /></span>
                <p class="empty-title">{tString('queue.empty.title')}</p>
                <p class="empty-body">{tString('queue.empty.body')}</p>
            </div>
        {:else}
            <div
                bind:this={gridEl}
                class="queue-list queue-grid"
                role="grid"
                aria-label={tString('queue.list.aria')}
                aria-multiselectable="true"
                onfocusin={() => {
                    gridOwnsFocus = true
                }}
                onfocusout={handleGridFocusOut}
            >
                {#each rows as row (row.snapshot.operationId)}
                    {@const id = row.snapshot.operationId}
                    <QueueRow
                        bind:this={rowApis[id]}
                        {row}
                        selected={selectedIds.has(id)}
                        isCursor={cursor.cursorId === id}
                        tabbable={tabStopId === id}
                        {treeCollapsible}
                        onFocusRow={() => {
                            if (cursor.cursorId !== id) cursor.place(id)
                        }}
                        onToggleSelect={() => {
                            toggleSelect(id)
                        }}
                        onDismiss={() => void dismissRow(id)}
                    />
                {/each}
            </div>
        {/if}
    {/if}
    <!-- Where a reordered operation landed. The list re-renders silently, so
         this is the only way a screen reader hears that ⌥↑ / ⌥↓ did anything. -->
    <div class="sr-only" role="status" aria-live="polite">{announcement}</div>
</main>

<style>
    .queue-window {
        width: 100%;
        height: 100vh;
        background: var(--color-bg-glass);
        color: var(--color-text-primary);
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
        overflow: hidden;
        display: flex;
        flex-direction: column;
        position: relative;
    }

    /* Drag strip over the overlay title-bar row (where the traffic lights sit). */
    .window-drag-region {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: var(--titlebar-height);
        z-index: var(--z-sticky);
    }

    .queue-header {
        /* Start below the overlay title-bar so the title and toolbar clear the
           traffic lights and the drag strip above. */
        padding: var(--spacing-sm) var(--spacing-md) var(--spacing-sm);
        padding-top: calc(var(--titlebar-height) + var(--spacing-xs));
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        border-bottom: 1px solid var(--color-border-glass);
    }

    .title-row {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: var(--spacing-md);
        /* Leave room for the traffic lights on the left of the drag strip. */
        padding-left: var(--spacing-xl);
    }

    .title {
        font-size: var(--font-size-lg);
        font-weight: 600;
    }

    .toolbar {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .toolbar-spacer {
        flex: 1;
    }

    .selected-count {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .btn-inner {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .queue-list {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-sm);
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        scrollbar-gutter: stable;
    }

    .empty-state {
        flex: 1;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xl);
        text-align: center;
    }

    .empty-icon {
        color: var(--color-text-tertiary);
        margin-bottom: var(--spacing-xs);
    }

    .empty-title {
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0;
    }

    .empty-body {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        max-width: 320px;
        margin: 0;
    }
</style>
