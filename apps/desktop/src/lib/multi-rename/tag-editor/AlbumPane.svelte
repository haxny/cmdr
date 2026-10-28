<script lang="ts">
    /**
     * The Tags window's left pane: the tags the files share, one row per field (Name | Value), drawn with the file
     * panes' own row classes (`app-file-list.css`), row height (the density setting) and font, so it reads exactly
     * like a file pane. The cursor moves with the list keys; F2 or Enter (`tags.edit`) edits the cursor row's value
     * in place (`TagValueEditor`), and Tab (`tags.switchPane`) goes to the Tracks pane. A row whose files differ
     * holds «multiple values» followed by every value; it sends nothing while the marker (or the «…and N more» tail)
     * is in the text, and once they are deleted the text left is set on every file (`parseSharedEdit`), or on the
     * selected tracks alone while the Tracks pane has a selection (`scope`: the rows then sum up those tracks, the
     * header says "5 of 42 tracks", and so does the editor's name). The cursor row's Clear and Values… sit under the
     * list; the cover is the last group, inside the list's scroll after the rows.
     */
    import type { Snippet } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { SharedRow, TagField } from '#lib/ipc/bindings.js'
    import { getRowHeight, getStripedRows } from '#lib/settings/reactive-settings.svelte.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import type { TagEdit } from '#lib/tauri-commands/index.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import Button from '#lib/ui/Button.svelte'
    import { FIELD_LABELS } from './field-labels'
    import { editorBox, onResize, paneCursorMove, revealRow, visibleRowCount, type EditorPlace } from './pane-cursor'
    import './tag-pane.css'
    import { formatFieldValue, isMultiLineField, multipleText, parseSharedEdit, sharedEditHint } from './shared-field-input'
    import { useScrollBoxSize, type ScrollBoxSize } from '#lib/utils/scroll-box-size-action.js'
    import TagValueEditor from './TagValueEditor.svelte'
    import type { TagsCommandId } from './tags-function-keys'
    import { dialogIsOpen, isClearKey, startsComposition, typedSeed } from './typed-key'
    import ValueChoices from './ValueChoices.svelte'

    interface Props {
        rows: SharedRow[]
        editable: boolean
        /** How many files carry a Date the writer would refuse; the Date row is red while it is above 0. */
        badDates?: number
        /** The tracks the rows sum up when the Tracks pane has a selection (`count` of `total`); `null` for all. */
        scope?: { count: number; total: number } | null
        /** This pane has focus: its cursor shows as the active one (`.is-focused`). */
        focused: boolean
        cursor: number
        onCursorChange: (index: number) => void
        onEdit: (edits: TagEdit[]) => void | Promise<void>
        /** Delete or Backspace on the cursor row: empties the field, in the scope Clear works in, without asking. */
        onClear: (target: { field: TagField; path: string | null }) => void
        onSwitchPane: () => void
        /** Whether this key press is the echo of one the native menu already delivered (`createTagsKeyEcho`). */
        isEcho?: (id: TagsCommandId) => boolean
        /** The cover group, last in the pane's scroll. */
        cover?: Snippet
    }

    const {
        rows,
        editable,
        badDates = 0,
        scope = null,
        focused,
        cursor,
        onCursorChange,
        onEdit,
        onClear,
        onSwitchPane,
        isEcho,
        cover,
    }: Props = $props()

    /** Fixed name column, flexing value: the pane never scrolls sideways. */
    const GRID = 'calc(110px * var(--font-scale)) minmax(0, 1fr)'
    const DATE_HINT_ID = 'tag-album-date-hint'
    const SCOPE_ID = 'tag-album-scope'

    /** The scope's words for a label ("5 of 42 tracks"), or `null` while the rows cover every file. */
    const scopeParams = $derived(
        scope === null
            ? null
            : { total: scope.total, countText: formatInteger(scope.count), totalText: formatInteger(scope.total) },
    )

    /** "5 of 42 tracks" while a selection narrows the rows; nothing while they cover every track. */
    const scopeText = $derived(scopeParams === null ? null : tString('multiRename.tags.album.scope', scopeParams))

    const rowHeight = $derived(getRowHeight())
    const striped = $derived(getStripedRows())

    const shown = $derived(
        rows.map((row) => {
            const { text, multiple } = formatFieldValue(row.value)
            return {
                field: row.field,
                label: tString(FIELD_LABELS[row.field]),
                choices: row.choices,
                empty: row.value === 'empty',
                multiple,
                more: row.choicesMore,
                text: multiple ? multipleText(row.choices, row.choicesMore) : text,
                invalid: row.field === 'date' && badDates > 0,
            }
        }),
    )

    /** The editor's name: what the typed value goes to, every file or the selected tracks. */
    function editLabel(field: string): string {
        if (scopeParams === null) return tString('multiRename.tags.album.editLabel', { field })
        return tString('multiRename.tags.album.editLabelScoped', { field, ...scopeParams })
    }

    /** The Clear button's name, in the same scope. */
    function clearLabel(field: string): string {
        if (scopeParams === null) return tString('multiRename.tags.clearRowLabel', { field })
        return tString('multiRename.tags.clearRowLabelScoped', { field, ...scopeParams })
    }

    const at = $derived(Math.max(0, Math.min(cursor, shown.length - 1)))
    const current = $derived(shown.at(at))

    let list = $state<HTMLElement>()
    let scroller = $state<HTMLElement>()
    /** The scroller's classic scroll bar: the header pads by it so its columns stay over the rows' (as `FullList`). */
    let scrollbarWidth = $state(0)
    const measureScroller = (box: ScrollBoxSize) => {
        scrollbarWidth = Math.max(0, box.offsetWidth - box.clientWidth)
    }
    let editing = $state<{ field: TagField; place: EditorPlace; seed?: string } | null>(null)
    const edited = $derived(editing === null ? undefined : shown.find((row) => row.field === editing?.field))

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

    /** F2 from the window's bar or with focus outside the pane: edits the cursor row's value. */
    export function editAtCursor(): void {
        openEditor(at)
    }

    /** Where the editor goes over row `index`'s value, or `null` when the row isn't drawn. */
    function placeAt(index: number): EditorPlace | null {
        const row = shown.at(index)
        const cell = list?.querySelector<HTMLElement>(`[data-index="${String(index)}"] .col-value`)
        if (row === undefined || !cell || !scroller) return null
        return editorBox(scroller, cell, isMultiLineField(row.field) ? null : rowHeight)
    }

    function openEditor(index: number, seed?: string): void {
        const row = shown.at(index)
        const place = placeAt(index)
        if (!editable || row === undefined || place === null) return
        editing = { field: row.field, place, seed }
    }

    // A resize reflows the flexing Value column: the open editor follows its cell.
    $effect(() => {
        if (!scroller) return
        return onResize(scroller, () => {
            const open = editing
            if (open === null) return
            const place = placeAt(shown.findIndex((row) => row.field === open.field))
            if (place !== null) editing = { ...open, place }
        })
    })

    /** A click on a row: the cursor goes there and the list keeps focus, so the keys go on working. */
    function onRowMouseDown(e: MouseEvent, index: number): void {
        e.preventDefault()
        list?.focus()
        onCursorChange(index)
    }

    function closeEditor(refocus: boolean): void {
        editing = null
        if (refocus) list?.focus()
    }

    function commit(row: (typeof shown)[number], typed: string): void | Promise<void> {
        const edit = parseSharedEdit(row.field, row.text, typed, row.more)
        if (edit) return onEdit([edit])
    }

    /** Typing on the cursor row opens its value holding the character; Delete / Backspace empty it. */
    function answerTypedKey(e: KeyboardEvent): boolean {
        if (dialogIsOpen() || !editable || current === undefined) return false
        if (startsComposition(e)) {
            // Not claimed: the OS composes the accented letter into the box that opens.
            openEditor(at, '')
            return true
        }
        const seed = typedSeed(e)
        const clear = seed === null && isClearKey(e)
        if (seed === null && !clear) return false
        claimKey(e)
        if (seed !== null) openEditor(at, seed)
        else if (!current.empty) onClear({ field: current.field, path: null })
        return true
    }

    function onkeydown(e: KeyboardEvent): void {
        if (e.isComposing || e.target !== list) return
        if (eventMatchesCommand(e, 'tags.switchPane')) {
            claimKey(e)
            onSwitchPane()
            return
        }
        if (eventMatchesCommand(e, 'tags.edit')) {
            claimKey(e)
            // Enter is no menu key, so only F2 can have arrived by the menu too.
            if (e.key === 'Enter' || isEcho?.('tags.edit') !== true) openEditor(at)
            return
        }
        if (answerTypedKey(e)) return
        const move = paneCursorMove(e, { count: shown.length, from: at, visibleRows: visibleRowCount(scroller, rowHeight) })
        if (move === null || move.extend) return
        claimKey(e)
        onCursorChange(move.to)
        revealRow(list, move.to)
    }
</script>

<div class="tag-pane full-list-container" class:is-focused={focused}>
    <div
        class="tag-pane-header"
        style:grid-template-columns={GRID}
        style:--spacing-scrollbar-width="{scrollbarWidth}px"
        aria-hidden="true"
    >
        <span class="cut">{tString('multiRename.tags.album.colName')}</span>
        <span class="value-head">
            <span class="cut">{tString('multiRename.tags.album.colValue')}</span>
            {#if scopeText !== null}
                <!-- Read out as the list's description: the header itself is hidden from screen readers. -->
                <span class="scope cut" id={SCOPE_ID}>{scopeText}</span>
            {/if}
        </span>
    </div>
    <!-- `data-cover-clip`: the cover section cuts its drop area to this scroller (`CoverPanel`). -->
    <div class="full-list" bind:this={scroller} use:useScrollBoxSize={{ onResize: measureScroller }} data-cover-clip>
        <div
            bind:this={list}
            class="listbox-region"
            role="listbox"
            tabindex="0"
            aria-label={tString('multiRename.tags.album.label')}
            aria-describedby={scopeText === null ? undefined : SCOPE_ID}
            aria-activedescendant={current ? `tag-album-row-${current.field}` : undefined}
            {onkeydown}
        >
            {#each shown as row, index (row.field)}
                <!-- Not focusable, as in `FullList`: the listbox holds focus and names the cursor row through
                     `aria-activedescendant`, so a click never strands the keys on a row. -->
                <!-- svelte-ignore a11y_interactive_supports_focus -->
                <div
                    id="tag-album-row-{row.field}"
                    role="option"
                    aria-selected={index === at}
                    aria-describedby={row.invalid ? DATE_HINT_ID : undefined}
                    class="file-entry"
                    class:is-under-cursor={index === at}
                    class:is-striped={striped && index % 2 === 1}
                    class:is-invalid={row.invalid}
                    data-index={index}
                    style:height="{rowHeight}px"
                    style:grid-template-columns={GRID}
                    onmousedown={(e: MouseEvent) => { onRowMouseDown(e, index) }}
                    ondblclick={() => { openEditor(index) }}
                >
                    <span class="col-name cut">{row.label}</span>
                    <span class="col-value cut" class:is-multiple={row.multiple} use:tooltip={{ text: row.text, overflowOnly: true }}>{row.text}</span>
                </div>
            {/each}
        </div>
        {#if editing !== null && edited !== undefined}
            {@const row = edited}
            <TagValueEditor
                bind:this={editor}
                value={row.text}
                seed={editing.seed}
                ariaLabel={editLabel(row.label)}
                multiline={isMultiLineField(row.field)}
                invalid={row.invalid}
                describedBy={row.invalid ? DATE_HINT_ID : undefined}
                place={editing.place}
                blockedHint={(typed: string) => sharedEditHint(typed, row.more)}
                onCommit={(typed: string) => commit(row, typed)}
                onClose={closeEditor}
            />
        {/if}
        {#if cover}
            <!-- The cover is the pane's last group, in its scroll after the rows, so it never pushes or resizes the
                 list; `CoverPanel` carries its own heading. -->
            <div class="cover-group">{@render cover()}</div>
        {/if}
    </div>
    {#if shown.some((row) => row.invalid)}
        <p class="hint" id={DATE_HINT_ID}>{tString('multiRename.tags.date.invalid')}</p>
    {/if}
    {#if current !== undefined}
        {@const row = current}
        <div class="row-actions">
            {#if row.multiple && row.choices.length > 0}
                <ValueChoices
                    field={row.label}
                    choices={row.choices}
                    disabled={!editable}
                    onPick={(values: string[]) => { void onEdit([{ kind: 'setAll', field: row.field, values }]) }}
                />
            {/if}
            <Button
                size="mini"
                disabled={!editable || row.empty}
                aria-label={clearLabel(row.label)}
                onclick={() => { void onEdit([{ kind: 'setAll', field: row.field, values: [] }]) }}
            >
                {tString('multiRename.tags.clearRow')}
            </Button>
        </div>
    {/if}
</div>

<style>
    /* The pane's box (container, header, scroller, gutter, row) is the shared `tag-pane.css`, the file panes' Full
       view; the fills are the global `app-file-list.css`'s. Only this pane's own cells are styled here. */
    .col-name {
        color: var(--color-text-secondary);
    }

    .col-value.is-multiple {
        color: var(--color-text-secondary);
        font-style: italic;
    }

    .file-entry.is-invalid .col-name,
    .file-entry.is-invalid .col-value,
    .hint {
        color: var(--color-error-text);
    }

    .hint {
        margin: 0;
        padding: var(--spacing-xxs) var(--spacing-md);
        font-size: var(--font-size-sm);
    }

    /* The Value header, and "5 of 42 tracks" at its right while a selection narrows the rows. */
    .value-head {
        display: flex;
        justify-content: space-between;
        gap: var(--spacing-sm);
        min-width: 0;
    }

    .scope {
        flex-shrink: 0;
    }

    .row-actions {
        display: flex;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-md);
        border-top: 1px solid var(--color-border-subtle);
    }

    .cover-group {
        padding: var(--spacing-xs) var(--spacing-md) var(--spacing-sm);
        border-top: 1px solid var(--color-border-subtle);
    }
</style>
