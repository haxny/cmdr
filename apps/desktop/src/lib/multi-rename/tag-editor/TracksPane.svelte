<script lang="ts">
    /**
     * The Tags window's right pane: the session's audio files in its order, ☐ ▶ # Title Length File, drawn with the
     * file panes' row classes, row height and font. Selection works as it does on files (`classifySelectionKey` over
     * `track-selection.svelte.ts`, which is `createSelectionState`): Space toggles, Insert toggles and moves down,
     * ⇧ + a cursor key extends, ⌘A selects all; the ☐ column shows it, and the tag tools work on it. F2
     * (`tags.edit`) edits the cursor row's Title in place, ⇧F2 (`tags.editNumber`) its #, F3 or ⌥Space
     * (`tags.play`) plays it, and Tab (`tags.switchPane`) goes to the Album pane. A single-file book has
     * no # column: it has no track number.
     */
    import type { Snippet } from 'svelte'
    import { classifySelectionKey } from '#lib/file-explorer/pane/selection-keys.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { TrackRow, TrackSort, TrackSortKey } from '#lib/ipc/bindings.js'
    import { getRowHeight, getStripedRows } from '#lib/settings/reactive-settings.svelte.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import type { TagEdit } from '#lib/tauri-commands/index.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import { useScrollBoxSize, type ScrollBoxSize } from '#lib/utils/scroll-box-size-action.js'
    import { editorBox, onResize, paneCursorMove, revealRow, visibleRowCount, type EditorPlace } from './pane-cursor'
    import './tag-pane.css'
    import { formatTrackLength, parseFieldText } from './shared-field-input'
    import TagValueEditor from './TagValueEditor.svelte'
    import TrackSortHeader from './TrackSortHeader.svelte'
    import type { TagsCommandId } from './tags-function-keys'
    import type { TrackSelection } from './track-selection.svelte'

    interface Props {
        tracks: TrackRow[]
        editable: boolean
        focused: boolean
        cursor: number
        onCursorChange: (index: number) => void
        selection: TrackSelection
        /** The path of the track playing now, or `null`. */
        playing: string | null
        /** Plays the track at `path`, or stops it when it is the one playing. */
        onPlay: (path: string) => void
        onEdit: (edits: TagEdit[]) => void | Promise<void>
        onSwitchPane: () => void
        /** The order the tracks are in, or `null` when none of the headers' orders is (Multi-Rename moved a row). */
        sort: TrackSort | null
        /** A header was clicked: the order to put the tracks in. */
        onSort: (sort: TrackSort) => void
        /** Under the count, at the pane's foot (Open in foobar2000). */
        footer?: Snippet
        /** Whether this key press is the echo of one the native menu already delivered (`createTagsKeyEcho`). */
        isEcho?: (id: TagsCommandId) => boolean
    }

    const {
        tracks,
        editable,
        focused,
        cursor,
        onCursorChange,
        selection,
        playing,
        onPlay,
        onEdit,
        onSwitchPane,
        sort,
        onSort,
        footer,
        isEcho,
    }: Props = $props()

    type EditedField = 'title' | 'track'

    const rowHeight = $derived(getRowHeight())
    const striped = $derived(getStripedRows())
    const showNumber = $derived(tracks.length !== 1)
    /** Fixed columns, flexing Title: the pane never scrolls sideways. */
    const grid = $derived(
        [
            // The header's select-all Checkbox is 16px whatever the font scale, so this column is too.
            '16px',
            'calc(14px * var(--font-scale))',
            ...(showNumber ? ['calc(32px * var(--font-scale))'] : []),
            'minmax(0, 1fr)',
            'calc(52px * var(--font-scale))',
            'calc(170px * var(--font-scale))',
        ].join(' '),
    )
    const at = $derived(Math.max(0, Math.min(cursor, tracks.length - 1)))
    const allSelected = $derived(tracks.length > 0 && selection.count === tracks.length)

    let list = $state<HTMLElement>()
    let scroller = $state<HTMLElement>()
    /** The scroller's classic scroll bar: the header pads by it so its columns stay over the rows' (as `FullList`). */
    let scrollbarWidth = $state(0)
    const measureScroller = (box: ScrollBoxSize) => {
        scrollbarWidth = Math.max(0, box.offsetWidth - box.clientWidth)
    }
    let editing = $state<{ path: string; field: EditedField; place: EditorPlace } | null>(null)
    const edited = $derived(editing === null ? undefined : tracks.find((track) => track.path === editing?.path))

    /** Gives the pane focus (Tab from the other pane). */
    export function focus(): void {
        list?.focus()
    }

    let editor = $state<{ commit: () => boolean }>()

    /**
     * Before another command acts: sends the open box's text and closes it. `false` when the text can't be sent; the
     * box then stays open with its hint, and the caller doesn't act.
     */
    export function settleEditor(): boolean {
        return editing === null || (editor?.commit() ?? true)
    }

    /** F2 / ⇧F2 from the window's bar or with focus outside the pane: the cursor row's Title or #. */
    export function editAtCursor(field: EditedField): void {
        if (field === 'track' && !showNumber) return
        openEditor(at, field)
    }

    /** F3 from the window's bar or with focus outside the pane: plays or stops the cursor row. */
    export function playAtCursor(): void {
        play(at)
    }

    /** A header click: the active column flips its direction, another column starts ascending. */
    function sortBy(key: TrackSortKey): void {
        onSort({ key, descending: sort?.key === key && !sort.descending })
    }

    function canEdit(track: TrackRow | undefined): track is TrackRow {
        return editable && track !== undefined && track.readError === null
    }

    /** Where the editor goes over row `index`'s Title or #, or `null` when that cell isn't drawn. */
    function placeAt(index: number, field: EditedField): EditorPlace | null {
        const column = field === 'title' ? 'title' : 'number'
        const cell = list?.querySelector<HTMLElement>(`[data-index="${String(index)}"] [data-col="${column}"]`)
        if (!cell || !scroller) return null
        return editorBox(scroller, cell, rowHeight)
    }

    function openEditor(index: number, field: EditedField): void {
        const track = tracks.at(index)
        const place = placeAt(index, field)
        if (!canEdit(track) || place === null) return
        editing = { path: track.path, field, place }
    }

    // A resize reflows the flexing Title column: the open editor follows its cell.
    $effect(() => {
        if (!scroller) return
        return onResize(scroller, () => {
            const open = editing
            if (open === null) return
            const place = placeAt(tracks.findIndex((track) => track.path === open.path), open.field)
            if (place !== null) editing = { ...open, place }
        })
    })

    function closeEditor(refocus: boolean): void {
        editing = null
        if (refocus) list?.focus()
    }

    function commit(path: string, field: EditedField, text: string): void | Promise<void> {
        return onEdit([{ kind: 'setOne', path, field, values: parseFieldText(field, text) }])
    }

    function play(index: number): void {
        const track = tracks.at(index)
        if (track !== undefined && track.readError === null) onPlay(track.path)
    }

    function moveTo(index: number): void {
        onCursorChange(index)
        revealRow(list, index)
    }

    /** Space, Insert, ⌘A, ⌘⇧A, and * as in a file pane. Returns whether the key was one of them. */
    function answerSelectionKey(e: KeyboardEvent): boolean {
        const command = classifySelectionKey(e)
        if (command === null || command === 'selection.selectSameKind') return false
        claimKey(e)
        if (command === 'selection.toggle') selection.toggleAt(at)
        else if (command === 'selection.toggleAndDown') {
            selection.toggleAt(at)
            moveTo(Math.min(at + 1, tracks.length - 1))
        } else if (command === 'selection.selectAll') selection.selectAll()
        else if (command === 'selection.deselectAll') selection.deselectAll()
        else selection.invert()
        return true
    }

    /** The command a pane key means: Tab, F3 / ⌥Space, F2 / Enter, ⇧F2. */
    function paneCommandOf(e: KeyboardEvent): TagsCommandId | 'tags.switchPane' | null {
        const ids = ['tags.switchPane', 'tags.play', 'tags.edit', 'tags.editNumber'] as const
        return ids.find((id) => eventMatchesCommand(e, id)) ?? null
    }

    /**
     * The pane's own commands. Returns whether the key was one of them. Claimed even when it does nothing (⌥Space on
     * an unreadable row: macOS would type a non-breaking space), and dropped when it's the echo of a menu key.
     */
    function answerPaneKey(e: KeyboardEvent): boolean {
        const id = paneCommandOf(e)
        if (id === null) return false
        claimKey(e)
        if (id === 'tags.switchPane') {
            onSwitchPane()
            return true
        }
        // Enter and ⌥Space are no menu keys, so only F2, F3 and ⇧F2 can have arrived by the menu too.
        const viaMenuToo = e.key !== 'Enter' && e.code !== 'Space'
        if (viaMenuToo && isEcho?.(id) === true) return true
        if (id === 'tags.play') play(at)
        else if (id === 'tags.edit') openEditor(at, 'title')
        else if (showNumber) openEditor(at, 'track')
        return true
    }

    function onkeydown(e: KeyboardEvent): void {
        if (e.isComposing || e.target !== list || tracks.length === 0) return
        if (answerPaneKey(e) || answerSelectionKey(e)) return
        const move = paneCursorMove(e, { count: tracks.length, from: at, visibleRows: visibleRowCount(scroller, rowHeight) })
        if (move === null) return
        claimKey(e)
        if (move.extend) selection.extend({ from: at, to: move.to, overflow: move.overflow })
        moveTo(move.to)
    }

    /** A double-click edits the number on #, else the Title (also from Length and File). ☐ and ▶ open nothing. */
    function onRowDoubleClick(e: MouseEvent, index: number): void {
        const column = e.target instanceof Element ? e.target.closest<HTMLElement>('[data-col]')?.dataset.col : undefined
        if (column === 'mark' || column === 'play') return
        openEditor(index, column === 'number' ? 'track' : 'title')
    }

    /**
     * A click on a row: the cursor goes there and the list keeps focus, so the keys go on working; ⌘ toggles the row
     * and ⇧ selects up to it, as on files. On the ☐ the click itself toggles, so ⌘ adds nothing there.
     */
    function onRowMouseDown(e: MouseEvent, index: number): void {
        e.preventDefault()
        list?.focus()
        const onMark = e.target instanceof Element && e.target.closest('[data-col="mark"]') !== null
        if (e.metaKey && !onMark) selection.toggleAt(index)
        else if (e.shiftKey) selection.extendTo(index, at)
        onCursorChange(index)
    }
</script>

<div class="tag-pane full-list-container" class:is-focused={focused}>
    <div
        class="tag-pane-header"
        role="group"
        aria-label={tString('multiRename.tags.sort.label')}
        style:grid-template-columns={grid}
        style:--spacing-scrollbar-width="{scrollbarWidth}px"
    >
        <span class="head-mark">
            <Checkbox
                ariaLabel={tString('multiRename.tags.select.all')}
                checked={allSelected}
                indeterminate={selection.count > 0 && !allSelected}
                disabled={tracks.length === 0}
                onCheckedChange={() => { selection.setAll(!allSelected) }}
            />
        </span>
        <span class="sr-only">{tString('multiRename.tags.col.play')}</span>
        {#if showNumber}
            <TrackSortHeader sortKey="discTrack" label={tString('multiRename.tags.col.number')} current={sort} onClick={sortBy} align="right" />
        {/if}
        <TrackSortHeader sortKey="title" label={tString('multiRename.tags.col.title')} current={sort} onClick={sortBy} />
        <TrackSortHeader sortKey="length" label={tString('multiRename.tags.col.length')} current={sort} onClick={sortBy} align="right" />
        <TrackSortHeader sortKey="fileName" label={tString('multiRename.tags.col.fileName')} current={sort} onClick={sortBy} />
    </div>
    <div class="full-list" bind:this={scroller} use:useScrollBoxSize={{ onResize: measureScroller }}>
        <div
            bind:this={list}
            class="listbox-region"
            role="listbox"
            tabindex="0"
            aria-multiselectable="true"
            aria-label={tString('multiRename.tags.tracks.label')}
            aria-activedescendant={tracks.length > 0 ? `tag-track-row-${String(at)}` : undefined}
            {onkeydown}
        >
            {#each tracks as track, index (track.path)}
                {@const selected = selection.has(index)}
                <!-- Not focusable, as in `FullList`: the listbox holds focus and names the cursor row through
                     `aria-activedescendant`, so a click never strands the keys on a row. -->
                <!-- svelte-ignore a11y_interactive_supports_focus -->
                <div
                    id="tag-track-row-{index}"
                    role="option"
                    aria-selected={selected}
                    class="file-entry"
                    class:is-under-cursor={index === at}
                    class:is-selected={selected}
                    class:is-striped={striped && index % 2 === 1}
                    class:is-unreadable={track.readError !== null}
                    class:is-changed={track.changed}
                    data-index={index}
                    data-track-path={track.path}
                    style:height="{rowHeight}px"
                    style:grid-template-columns={grid}
                    onmousedown={(e: MouseEvent) => { onRowMouseDown(e, index) }}
                    ondblclick={(e: MouseEvent) => { onRowDoubleClick(e, index) }}
                >
                    <!-- Mouse shortcuts beside the keys (Space selects, ⌥Space plays); hidden from screen readers,
                         which hear the selection on the option and "Playing" at the row's end. -->
                    <span class="mark" class:on={selected} data-col="mark" aria-hidden="true" onclick={() => { selection.toggleAt(index) }}></span>
                    <span class="play" class:on={playing === track.path} data-col="play" aria-hidden="true" onclick={() => { play(index) }}
                        >{track.readError !== null ? '' : playing === track.path ? '■' : '▶'}</span
                    >
                    {#if showNumber}<span class="cut end" data-col="number">{track.track ?? ''}</span>{/if}
                    <span class="cut" data-col="title" use:tooltip={{ text: track.title ?? '', overflowOnly: true }}>{track.title ?? ''}</span>
                    <span class="cut end length" data-col="length">{formatTrackLength(track.durationMs)}</span>
                    <span class="cut file" data-col="file" use:tooltip={{ text: track.fileName, overflowOnly: true }}
                        >{track.fileName}{#if track.readError !== null}<span class="sr-only">, {tString('multiRename.tags.track.unreadable')}</span>{/if}{#if playing === track.path}<span class="sr-only">, {tString('multiRename.tags.track.playing')}</span>{/if}</span
                    >
                </div>
            {/each}
        </div>
        {#if editing !== null && edited !== undefined}
            {@const track = edited}
            {@const field = editing.field}
            <TagValueEditor
                bind:this={editor}
                value={(field === 'title' ? track.title : track.track) ?? ''}
                ariaLabel={tString(field === 'title' ? 'multiRename.tags.track.titleLabel' : 'multiRename.tags.track.numberLabel', { fileName: track.fileName })}
                place={editing.place}
                onCommit={(text: string) => commit(track.path, field, text)}
                onClose={closeEditor}
            />
        {/if}
    </div>
    <div class="foot">
        <span class="count">
            {tString('multiRename.tags.trackCount', { count: tracks.length, countText: formatInteger(tracks.length) })}
        </span>
        {@render footer?.()}
    </div>
</div>

<style>
    /* The pane's box (container, header, scroller, gutter, row) is the shared `tag-pane.css`, the file panes' Full
       view; the fills are the global `app-file-list.css`'s. Only this pane's own cells are styled here. */
    .head-mark {
        display: flex;
        align-items: center;
    }

    .end {
        text-align: right;
    }

    .length {
        font-variant-numeric: tabular-nums;
    }

    .file {
        color: var(--color-text-secondary);
    }

    .file-entry.is-changed .file {
        color: var(--color-text-primary);
    }

    .file-entry.is-unreadable {
        color: var(--color-text-tertiary);
    }

    /* The ☐ column: the row's selection, drawn as a small box. */
    .mark {
        box-sizing: border-box;
        width: 0.9em;
        height: 0.9em;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .mark.on {
        border-color: var(--color-selection-fg);
        background: var(--color-selection-fg);
    }

    .play {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        text-align: center;
    }

    .play.on {
        color: var(--color-accent-text);
    }

    .foot {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-md);
        border-top: 1px solid var(--color-border-subtle);
    }

    .count {
        flex: 1;
        color: var(--color-text-secondary);
    }
</style>
