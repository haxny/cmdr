<script lang="ts">
    /**
     * The Tags window's right pane: the session's audio files in its order, ☐ ▶ # Title Length File, drawn with the
     * file panes' row classes, row height and font. Selection works as it does on files (`classifySelectionKey` over
     * `track-selection.svelte.ts`, which is `createSelectionState`): Space toggles, Insert toggles and moves down,
     * ⇧ + a cursor key extends, ⌘A selects all; the ☐ column shows it, and the tag tools work on it. F2
     * (`tags.edit`) edits the cursor row's Title in place, ⇧F2 (`tags.editNumber`) its #, ⌥Space
     * (`multiRename.playTrack`) plays it, and Tab (`tags.switchPane`) goes to the Album pane. A single-file book has
     * no # column: it has no track number.
     */
    import type { Snippet } from 'svelte'
    import { classifySelectionKey } from '#lib/file-explorer/pane/selection-keys.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { TrackRow } from '#lib/ipc/bindings.js'
    import { getRowHeight, getStripedRows } from '#lib/settings/reactive-settings.svelte.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import type { TagEdit } from '#lib/tauri-commands/index.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import { editorBox, paneCursorMove, revealRow, visibleRowCount } from './pane-cursor'
    import { formatTrackLength, parseFieldText } from './shared-field-input'
    import TagValueEditor from './TagValueEditor.svelte'
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
        /** Under the count, at the pane's foot (Open in foobar2000). */
        footer?: Snippet
    }

    const { tracks, editable, focused, cursor, onCursorChange, selection, playing, onPlay, onEdit, onSwitchPane, footer }: Props =
        $props()

    type EditedField = 'title' | 'track'

    const rowHeight = $derived(getRowHeight())
    const striped = $derived(getStripedRows())
    const showNumber = $derived(tracks.length !== 1)
    /** Fixed columns, flexing Title: the pane never scrolls sideways. */
    const grid = $derived(
        [
            'calc(14px * var(--font-scale))',
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
    let editing = $state<{ path: string; field: EditedField; style: string } | null>(null)
    const edited = $derived(editing === null ? undefined : tracks.find((track) => track.path === editing?.path))

    /** Gives the pane focus (Tab from the other pane). */
    export function focus(): void {
        list?.focus()
    }

    function canEdit(track: TrackRow | undefined): track is TrackRow {
        return editable && track !== undefined && track.readError === null
    }

    function openEditor(index: number, field: EditedField): void {
        const track = tracks.at(index)
        const column = field === 'title' ? 'title' : 'number'
        const cell = list?.querySelector<HTMLElement>(`[data-index="${String(index)}"] [data-col="${column}"]`)
        if (!canEdit(track) || !cell || !scroller) return
        editing = { path: track.path, field, style: editorBox(scroller, cell, rowHeight) }
    }

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

    /** The pane's own commands: Tab, ⌥Space, F2, ⇧F2. Returns whether the key was one of them. */
    function answerPaneKey(e: KeyboardEvent): boolean {
        if (eventMatchesCommand(e, 'tags.switchPane')) {
            claimKey(e)
            onSwitchPane()
        } else if (eventMatchesCommand(e, 'multiRename.playTrack')) {
            // Claimed even on an unreadable row: macOS would type a non-breaking space.
            claimKey(e)
            play(at)
        } else if (eventMatchesCommand(e, 'tags.edit')) {
            claimKey(e)
            openEditor(at, 'title')
        } else if (eventMatchesCommand(e, 'tags.editNumber')) {
            claimKey(e)
            if (showNumber) openEditor(at, 'track')
        } else return false
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

    /** A click on a row: the cursor goes there; ⌘ toggles it and ⇧ selects up to it, as on files. */
    function onRowMouseDown(e: MouseEvent, index: number): void {
        if (e.metaKey) selection.toggleAt(index)
        else if (e.shiftKey) selection.extendTo(index, at)
        onCursorChange(index)
    }
</script>

<div class="pane full-list-container" class:is-focused={focused}>
    <div class="header-row" style:grid-template-columns={grid}>
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
        {#if showNumber}<span class="cut">{tString('multiRename.tags.col.number')}</span>{/if}
        <span class="cut">{tString('multiRename.tags.col.title')}</span>
        <span class="cut end">{tString('multiRename.tags.col.length')}</span>
        <span class="cut">{tString('multiRename.tags.col.fileName')}</span>
    </div>
    <div class="full-list" bind:this={scroller}>
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
                <div
                    id="tag-track-row-{index}"
                    role="option"
                    tabindex="-1"
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
                    ondblclick={() => { openEditor(index, 'title') }}
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
                value={(field === 'title' ? track.title : track.track) ?? ''}
                ariaLabel={tString(field === 'title' ? 'multiRename.tags.track.titleLabel' : 'multiRename.tags.track.numberLabel', { fileName: track.fileName })}
                style={editing.style}
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
    /* The file panes' Full view, as in `AlbumPane.svelte`: `FullList.svelte`'s container, header, scroller, gutter,
       and row box; the stripe, selection, and cursor fills are the global `app-file-list.css`'s. */
    .pane {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
        min-width: 0;
        background-color: var(--color-bg-primary);
    }

    .header-row {
        display: grid;
        gap: var(--spacing-sm);
        padding: var(--spacing-xxs) var(--spacing-md) var(--spacing-xs);
        height: calc(22px * var(--font-scale) + var(--spacing-xs));
        align-items: center;
        flex-shrink: 0;
        overflow-x: hidden;
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        user-select: none;
    }

    .head-mark {
        display: flex;
        align-items: center;
    }

    .full-list {
        /* The containing block for the in-place editor, so it scrolls with its row. */
        position: relative;
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        overflow-x: hidden;
        font-family: var(--font-system), sans-serif;
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-flat);
    }

    .listbox-region {
        outline: none;
        padding: var(--spacing-xs);
    }

    .file-entry {
        display: grid;
        box-sizing: border-box;
        padding: var(--spacing-xxs) var(--spacing-sm);
        gap: var(--spacing-sm);
        align-items: center;
        white-space: nowrap;
        cursor: default;
    }

    /* Long titles and file names are cut with …; the tooltip shows the whole text. The block padding is FullList's
       descender room for clipped text. */
    .cut {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        padding-block: 0.2em;
        margin-block: -0.2em;
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
