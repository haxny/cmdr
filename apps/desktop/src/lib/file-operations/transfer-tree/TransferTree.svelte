<!--
    The live transfer tree: every selected file and folder with its own
    progress, as an ARIA tree over a virtual window. Shared by the progress
    dialog and the queue window's running rows.

    It pulls one window from its operation (or a fixture's `source`) on mount
    and again on each `tick` (the operation's `write-progress`), at most five a
    second and one request in flight (`transfer-tree-view.svelte.ts`). It follows the work
    until the user scrolls, wheels, or moves by keyboard; the Follow button (or
    F) resumes. The open folders scrolled past stay pinned at the top. It's as
    tall as its rows, up to `defaultRows`, and grows with a host that gives it
    more. `DETAILS.md` has the keyboard model, the follow mechanics, and the
    pinned headers.
-->
<script lang="ts">
    import { onDestroy, tick as afterRender, untrack } from 'svelte'
    import type { TransferTreeRow as Row } from '#lib/tauri-commands/index.js'
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { revealScrollTop, visibleRange } from '#lib/ui/column-list-layout.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { dependOn } from '#lib/utils/reactivity.js'
    import TransferTreeRow from './TransferTreeRow.svelte'
    import { createTransferTreeViewState, type TransferTreeMemory } from './transfer-tree-view.svelte'
    import { MAX_PINNED, pinnedHeaders } from './pinned-headers'
    import { rowPaths, type TransferTreeOperation } from './row-format'
    import { ipcTransferTreeSource, type TransferTreeSource } from './transfer-tree-source'

    interface CommonProps {
        /** Anything that changes when the operation advanced (the session's `progress`): each change pulls a fresh window. */
        tick: unknown
        operation: TransferTreeOperation
        /** Rows tall at most before the host gives it more room (the dialog's 8, the queue's 6), never below
         *  three. Fewer rows take less: a single file is one row. */
        defaultRows?: number
        /** Whether Tab reaches the tree (its cursor row, then Follow). The queue clears it on every
         *  row but the grid's cursor row, so the grid stays one Tab stop plus that row's tree. */
        tabbable?: boolean
        /** Where the host keeps this tree while it's away (the dialog's conflict prompt takes its
         *  place), so the tree that comes back is the one the user left. */
        memory?: TransferTreeMemory
    }

    /** A running operation by id, or a scripted fixture. Either is fixed for the tree's life. */
    type Props = CommonProps &
        ({ operationId: string; source?: never } | { source: TransferTreeSource; operationId?: never })

    const { tick, operation, defaultRows = 8, tabbable = true, operationId, source, memory }: Props = $props()

    /** The physical key that resumes following (like the decision prompts' letters, never translated). */
    const FOLLOW_KEY_CODE = 'KeyF'
    const FOLLOW_KEY = 'F'
    /** DOM rows kept beyond the viewport on each side. */
    const OVERSCAN = 4

    function initialSource(): TransferTreeSource {
        return source ?? ipcTransferTreeSource(operationId)
    }

    /** What this tree views, fixed for its life: the key it's remembered by. */
    const memoryKey: unknown = untrack(() => operationId ?? source)
    const remembered = untrack(() => memory?.recall(memoryKey) ?? null)
    const tree = createTransferTreeViewState(untrack(initialSource), { initial: remembered?.view })
    tree.setViewportRows(untrack(() => defaultRows))
    /** A paused tree's scroll position, put back once there's a layout (following puts its own). */
    let restoreTop: number | null = remembered && !remembered.view.following ? remembered.view.topRow : null
    onDestroy(() => {
        untrack(() => memory)?.keep(memoryKey, { view: tree.snapshot(), contentRows })
        tree.dispose()
    })

    $effect(() => {
        dependOn(tick)
        untrack(() => {
            tree.refresh()
        })
    })

    let scroller: HTMLDivElement | undefined = $state()
    let treeEl: HTMLDivElement | undefined = $state()
    let probe: HTMLDivElement | undefined = $state()
    let rowHeight = $state(0)
    let viewportHeight = $state(0)
    let scrollTop = $state(0)

    /** The keyboard cursor: a flat index, and the row it holds, followed by id as rows shift. */
    let focusIndex = $state<number | null>(null)
    let focusRowCache = $state.raw<Row | null>(null)
    let focusId: number | null = null
    let treeHasFocus = false
    /** A keyboard target whose row hadn't loaded when the key was pressed. */
    let pendingFocus: number | null = null

    const view = $derived(tree.view)
    const ready = $derived(view !== null && view.ready)
    const total = $derived(ready && view ? view.totalRows : 0)
    const measured = $derived(rowHeight > 0 && viewportHeight > 0)
    const paths = $derived(view ? rowPaths(view.rows, view.ancestors) : [])

    /** The tallest the content has asked for (capped at `defaultRows`): it grows as rows
     *  arrive but never shrinks back, so a folder shutting as the work moves on doesn't
     *  bounce the host's layout on every tick. */
    const maxRows = $derived(Math.max(3, defaultRows))
    let contentRows = $state(remembered?.contentRows ?? 1)
    $effect(() => {
        const wanted = Math.min(Math.max(total, 1), maxRows)
        if (wanted > untrack(() => contentRows)) contentRows = wanted
    })

    const topRow = $derived(measured ? Math.floor(scrollTop / rowHeight) : 0)
    /** The open folders scrolled past, pinned at the top (none before there's a layout). */
    const pinned = $derived(
        measured && ready ? pinnedHeaders(topRow, total, (index) => tree.ancestorsOf(index)) : [],
    )
    const pinnedPaths = $derived(pinned.map((_, level) => pinned.slice(0, level + 1).map((h) => h.row.label).join('/')))

    /** The rows on screen, or, before there's a layout (first frame, jsdom), the loaded window. */
    const range = $derived(
        measured
            ? visibleRange({ count: total, rowHeight, scrollTop, viewportHeight, overscan: OVERSCAN })
            : { start: view?.offset ?? 0, end: (view?.offset ?? 0) + (view?.rows.length ?? 0) },
    )

    function rowFor(index: number): Row | undefined {
        return tree.rowAt(index) ?? (index === focusIndex && focusRowCache !== null ? focusRowCache : undefined)
    }

    function pathFor(index: number, row: Row): string {
        return (view ? paths[index - view.offset] : undefined) ?? row.label
    }

    /** Every drawn index, plus the focused row wherever it scrolled to, so focus never drops. */
    const slots = $derived.by(() => {
        const out: number[] = []
        for (let index = range.start; index < Math.min(range.end, total); index++) out.push(index)
        if (focusIndex !== null && (focusIndex < range.start || focusIndex >= range.end) && rowFor(focusIndex)) {
            out.push(focusIndex)
        }
        return out
    })

    /** The one row Tab lands on: the cursor, else the work, else the first drawn row. */
    const tabStop = $derived.by(() => {
        if (focusIndex !== null && rowFor(focusIndex)) return focusIndex
        const work = view?.focusRow ?? null
        if (work !== null && slots.includes(work) && tree.rowAt(work)) return work
        return slots.find((index) => tree.rowAt(index) !== undefined) ?? null
    })

    const showFollow = $derived(ready && total > 0 && !tree.following && tree.ended === null)

    // Measure the viewport and one row (the probe carries the row-height token, so a
    // text-size change re-windows on its own).
    $effect(() => {
        const viewport = scroller
        const row = probe
        if (!viewport || !row) return
        const read = (): void => {
            viewportHeight = viewport.clientHeight
            rowHeight = row.getBoundingClientRect().height
            if (rowHeight > 0 && viewportHeight > 0) tree.setViewportRows(Math.ceil(viewportHeight / rowHeight))
        }
        read()
        if (typeof ResizeObserver === 'undefined') return
        const observer = new ResizeObserver(read)
        observer.observe(viewport)
        observer.observe(row)
        return () => {
            observer.disconnect()
        }
    })

    // Paused when it went away: back to where the user had scrolled.
    $effect(() => {
        const viewport = scroller
        if (restoreTop === null || !ready || rowHeight <= 0 || !viewport) return
        const target = restoreTop * rowHeight
        restoreTop = null
        untrack(() => {
            viewport.scrollTop = target
            scrollTop = viewport.scrollTop
        })
    })

    // Following: put the work where it belongs on every answer.
    $effect(() => {
        const top = tree.followTop
        const viewport = scroller
        if (top === null || rowHeight <= 0 || !viewport) return
        const target = top * rowHeight
        untrack(() => {
            if (Math.abs(viewport.scrollTop - target) >= 1) {
                viewport.scrollTop = target
                scrollTop = viewport.scrollTop
            }
        })
    })

    // The cursor follows its row by id when rows shift above it (a folder opening or
    // shutting as the work moves), and a pending keyboard target lands once loaded.
    $effect(() => {
        const current = tree.view
        if (!current?.ready) return
        untrack(() => {
            if (focusId !== null) {
                const at = current.rows.findIndex((row) => row.id === focusId)
                if (at >= 0) {
                    focusRowCache = current.rows[at]
                    const index = current.offset + at
                    if (index !== focusIndex) {
                        focusIndex = index
                        if (treeHasFocus) pendingFocus = index
                    }
                }
            } else if (focusIndex !== null) {
                const row = tree.rowAt(focusIndex)
                if (row) {
                    focusId = row.id
                    focusRowCache = row
                }
            }
            if (pendingFocus !== null && tree.rowAt(pendingFocus)) {
                const target = pendingFocus
                void afterRender().then(() => {
                    focusSlot(target)
                })
            }
        })
    })

    function focusSlot(index: number): void {
        const el = treeEl?.querySelector<HTMLElement>(`[data-index="${String(index)}"]`)
        if (!el) {
            pendingFocus = index
            return
        }
        pendingFocus = null
        el.focus({ preventScroll: true })
    }

    function onScroll(): void {
        if (!scroller) return
        scrollTop = scroller.scrollTop
        if (rowHeight > 0) tree.scrolledTo(Math.floor(scrollTop / rowHeight))
    }

    /** A wheel over rows that all fit scrolls the page around the tree (the queue window), not the tree. */
    function onWheel(event: WheelEvent): void {
        if (!scroller || scroller.scrollHeight <= scroller.clientHeight) return
        if (event.deltaY !== 0) tree.pauseFollowing()
    }

    /** A press on the scroller itself (not on a row) is its scrollbar: a user scroll. */
    function onPointerDown(event: PointerEvent): void {
        if (event.target === scroller) tree.pauseFollowing()
    }

    function reveal(index: number): void {
        if (!scroller || rowHeight <= 0) {
            // No layout to measure against: treat the target as the top of the view.
            tree.scrolledTo(index)
            return
        }
        // A row under the pinned stack is out of sight too: bring it to just below the
        // headers its own folders will make (one per level, up to the cap).
        const underStack = index * rowHeight < scroller.scrollTop + pinned.length * rowHeight
        const inset = Math.min(MAX_PINNED, rowFor(index)?.depth ?? 0)
        const next = underStack
            ? Math.max(0, (index - inset) * rowHeight)
            : revealScrollTop({
                  index,
                  rowHeight,
                  scrollTop: scroller.scrollTop,
                  viewportHeight: scroller.clientHeight,
              })
        if (next !== undefined) {
            scroller.scrollTop = next
            scrollTop = scroller.scrollTop
        }
        tree.scrolledTo(Math.floor(scrollTop / rowHeight))
    }

    function moveTo(index: number): void {
        if (total === 0) return
        const target = Math.max(0, Math.min(total - 1, index))
        tree.pauseFollowing()
        focusIndex = target
        const row = tree.rowAt(target)
        focusId = row?.id ?? null
        if (row) focusRowCache = row
        reveal(target)
        void afterRender().then(() => {
            focusSlot(target)
        })
    }

    function pageRows(): number {
        return measured ? Math.max(1, Math.floor(viewportHeight / rowHeight) - 1) : defaultRows - 1
    }

    /** Where each moving key takes the cursor from `at`. */
    const CURSOR_KEYS: Partial<Record<string, (at: number) => number>> = {
        ArrowDown: (at) => at + 1,
        ArrowUp: (at) => at - 1,
        PageDown: (at) => at + pageRows(),
        PageUp: (at) => at - pageRows(),
        Home: () => 0,
        End: () => total - 1,
    }

    /** Right: open a shut folder, or step into an open one. */
    function expandOrEnter(at: number): void {
        const row = rowFor(at)
        if (row?.kind !== 'folder' || !row.hasChildren) return
        if (row.expanded) moveTo(at + 1)
        else tree.toggle(row.id, false)
    }

    /** Left: shut an open folder, or step out to the parent. */
    function collapseOrLeave(at: number): void {
        const row = rowFor(at)
        if (row?.kind === 'folder' && row.expanded) {
            tree.toggle(row.id, true)
            return
        }
        const parent = tree.parentIndexOf(at)
        if (parent !== null) moveTo(parent)
    }

    function isFollowKey(event: KeyboardEvent): boolean {
        return event.code === FOLLOW_KEY_CODE && !event.shiftKey && !event.repeat && showFollow
    }

    function onKeydown(event: KeyboardEvent): void {
        if (event.metaKey || event.ctrlKey || event.altKey) return
        const at = focusIndex ?? tabStop ?? 0
        const cursorTo = CURSOR_KEYS[event.key]
        if (cursorTo) moveTo(cursorTo(at))
        else if (event.key === 'ArrowRight') expandOrEnter(at)
        else if (event.key === 'ArrowLeft') collapseOrLeave(at)
        else if (isFollowKey(event)) tree.resumeFollowing()
        else return
        event.preventDefault()
    }

    function rowIndexOf(target: EventTarget | null): number | null {
        const el = (target as HTMLElement | null)?.closest<HTMLElement>('[data-index]')
        return el ? Number(el.dataset.index) : null
    }

    function onClick(event: MouseEvent): void {
        const index = rowIndexOf(event.target)
        if (index === null || !(event.target as HTMLElement).closest('[data-chevron]')) return
        const row = rowFor(index)
        if (row) tree.toggle(row.id, row.expanded)
    }

    function onDoubleClick(event: MouseEvent): void {
        const index = rowIndexOf(event.target)
        const row = index === null ? undefined : rowFor(index)
        if (row?.kind === 'folder' && row.hasChildren) tree.toggle(row.id, row.expanded)
    }

    /** A header jumps to its folder's row; its chevron shuts the folder. */
    function onPinnedClick(event: MouseEvent): void {
        const el = (event.target as HTMLElement | null)?.closest<HTMLElement>('[data-pinned-index]')
        const header = el ? pinned.find((candidate) => candidate.index === Number(el.dataset.pinnedIndex)) : undefined
        if (!header) return
        if ((event.target as HTMLElement).closest('[data-chevron]')) tree.toggle(header.row.id, true)
        else moveTo(header.index)
    }

    function onFocusIn(event: FocusEvent): void {
        treeHasFocus = true
        const index = rowIndexOf(event.target)
        if (index === null || index === focusIndex) return
        focusIndex = index
        const row = rowFor(index)
        focusId = row?.id ?? null
        if (row) focusRowCache = row
    }

    function onFocusOut(event: FocusEvent): void {
        if (!treeEl?.contains(event.relatedTarget as Node | null)) treeHasFocus = false
    }
</script>

{#if tree.ended !== 'notATransfer'}
    <div class="transfer-tree" style:--spacing-tree-rows={contentRows}>
        <div class="probe" aria-hidden="true" bind:this={probe}></div>
        {#if !ready || total === 0}
            <p class="placeholder">
                {ready ? tString('fileOperations.transferTree.empty') : tString('fileOperations.transferTree.preparing')}
            </p>
        {:else if view}
            <!-- The scroller only listens for scroll intent (a wheel, a press on its scrollbar);
                 the tree inside it owns the keyboard. -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
                class="scroller"
                bind:this={scroller}
                onscroll={onScroll}
                onwheel={onWheel}
                onpointerdown={onPointerDown}
            >
                {#if pinned.length > 0}
                    <!-- Mirrors of real rows, so hidden from assistive tech: a screen
                         reader walks the tree, where Left already reaches every parent.
                         The pointer gets a click that jumps to the folder. -->
                    <div class="pinned" aria-hidden="true" onclick={onPinnedClick}>
                        {#each pinned as header, level (level)}
                            <div
                                class="pinned-slot"
                                class:last={level === pinned.length - 1}
                                style:top="calc({level} * var(--spacing-tree-row))"
                            >
                                <TransferTreeRow
                                    row={header.row}
                                    index={header.index}
                                    axis={view.axis}
                                    {operation}
                                    path={pinnedPaths[level]}
                                    tabStop={false}
                                    pinned
                                />
                            </div>
                        {/each}
                    </div>
                {/if}
                <div
                    class="rows"
                    role="tree"
                    aria-label={tString('fileOperations.transferTree.treeLabel')}
                    tabindex={tabbable && tabStop === null ? 0 : -1}
                    bind:this={treeEl}
                    style:height="calc({total} * var(--spacing-tree-row))"
                    onkeydown={onKeydown}
                    onclick={onClick}
                    ondblclick={onDoubleClick}
                    onfocusin={onFocusIn}
                    onfocusout={onFocusOut}
                >
                    {#each slots as index (index)}
                        {@const row = rowFor(index)}
                        {#if row}
                            <!-- Each row sits at its flat index, so the focused one can stay
                                 rendered wherever it scrolled to. -->
                            <div class="slot" role="none" style:top="calc({index} * var(--spacing-tree-row))">
                                <TransferTreeRow
                                    {row}
                                    {index}
                                    axis={view.axis}
                                    {operation}
                                    path={pathFor(index, row)}
                                    tabStop={tabbable && index === tabStop}
                                />
                            </div>
                        {/if}
                    {/each}
                </div>
            </div>
        {/if}
        {#if showFollow}
            <div class="follow">
                <Button
                    size="mini"
                    onclick={() => {
                        tree.resumeFollowing()
                    }}
                    aria-keyshortcuts={FOLLOW_KEY}
                    tabindex={tabbable ? undefined : -1}
                    tooltipContent={{
                        text: tString('fileOperations.transferTree.followTooltip'),
                        shortcut: FOLLOW_KEY,
                    }}
                >
                    <span class="follow-label"
                        ><Icon name="locate-fixed" size={12} aria-hidden="true" />{tString(
                            'fileOperations.transferTree.follow',
                        )}</span
                    >
                </Button>
            </div>
        {/if}
    </div>
{/if}

<style>
    .transfer-tree {
        /* A row: one line of small text plus breathing room, compact but not cramped. The
           probe carries it, so the virtual window divides by the real pixel height, and it
           scales with the text size. */
        --spacing-tree-row: calc(var(--font-size-sm) * var(--font-line-height-normal) + 2 * var(--spacing-xs));

        position: relative;
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        box-sizing: content-box;
        /* The rows the content asked for (set inline over this default); a resizable
           host may squeeze it to three of them, never fewer than it has. */
        --spacing-tree-rows: 1;

        height: calc(var(--spacing-tree-rows) * var(--spacing-tree-row));
        min-height: calc(min(var(--spacing-tree-rows), 3) * var(--spacing-tree-row));
        padding-block: var(--spacing-xxs);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        background: var(--color-bg-primary);
    }

    .probe {
        position: absolute;
        visibility: hidden;
        height: var(--spacing-tree-row);
        width: 1px;
        pointer-events: none;
    }

    .slot {
        position: absolute;
        inset-inline: 0;
        height: var(--spacing-tree-row);
    }

    .scroller {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        overscroll-behavior: contain;
    }

    /* Sticky at the scroller's top with no height of its own, so the rows start under it
       and the headers float over the ones they cover. */
    .pinned {
        position: sticky;
        inset-block-start: 0;
        z-index: 1;
        height: 0;
        margin-inline: var(--spacing-xxs);
    }

    .pinned-slot {
        position: absolute;
        box-sizing: border-box;
        inset-inline: 0;
        height: var(--spacing-tree-row);
        background: var(--color-bg-primary);
    }

    .pinned-slot.last {
        border-block-end: 1px solid var(--color-border-subtle);
    }

    .pinned-slot:hover {
        background: var(--color-bg-secondary);
    }

    .rows {
        position: relative;
        margin-inline: var(--spacing-xxs);
        outline: none;
    }

    .placeholder {
        flex: 1 1 auto;
        display: flex;
        align-items: center;
        justify-content: center;
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .follow {
        position: absolute;
        inset-block-end: var(--spacing-sm);
        inset-inline-end: var(--spacing-md);
        border-radius: var(--radius-input);
        box-shadow: var(--shadow-sm);
    }

    .follow-label {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
