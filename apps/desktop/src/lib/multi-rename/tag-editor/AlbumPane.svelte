<script lang="ts">
    /**
     * The Tags window's left pane: the tags the files share, one row per field (Name | Value), drawn with the file
     * panes' own row classes (`app-file-list.css`), row height (the density setting) and font, so it reads exactly
     * like a file pane. The cursor moves with the list keys; F2 or Enter (`tags.edit`) edits the cursor row's value
     * in place (`TagValueEditor`), and Tab (`tags.switchPane`) goes to the Tracks pane. A row whose files differ
     * holds «multiple values» followed by every value; it sends nothing while the marker (or the «…and N more» tail)
     * is in the text, and once they are deleted the text left is set on every file (`parseSharedEdit`). The cursor
     * row's Clear and Values… sit under the list; the cover is the last group.
     */
    import type { Snippet } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { SharedRow, TagField } from '#lib/ipc/bindings.js'
    import { getRowHeight, getStripedRows } from '#lib/settings/reactive-settings.svelte.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import type { TagEdit } from '#lib/tauri-commands/index.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import Button from '#lib/ui/Button.svelte'
    import { FIELD_LABELS } from './field-labels'
    import { editorBox, paneCursorMove, revealRow, visibleRowCount } from './pane-cursor'
    import { formatFieldValue, isMultiLineField, multipleText, parseSharedEdit } from './shared-field-input'
    import TagValueEditor from './TagValueEditor.svelte'
    import ValueChoices from './ValueChoices.svelte'

    interface Props {
        rows: SharedRow[]
        editable: boolean
        /** How many files carry a Date the writer would refuse; the Date row is red while it is above 0. */
        badDates?: number
        /** This pane has focus: its cursor shows as the active one (`.is-focused`). */
        focused: boolean
        cursor: number
        onCursorChange: (index: number) => void
        onEdit: (edits: TagEdit[]) => void | Promise<void>
        onSwitchPane: () => void
        /** The cover group, last in the pane. */
        cover?: Snippet
    }

    const { rows, editable, badDates = 0, focused, cursor, onCursorChange, onEdit, onSwitchPane, cover }: Props = $props()

    /** Fixed name column, flexing value: the pane never scrolls sideways. */
    const GRID = 'calc(110px * var(--font-scale)) minmax(0, 1fr)'
    const DATE_HINT_ID = 'tag-album-date-hint'

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
    const at = $derived(Math.max(0, Math.min(cursor, shown.length - 1)))
    const current = $derived(shown.at(at))

    let list = $state<HTMLElement>()
    let scroller = $state<HTMLElement>()
    let editing = $state<{ field: TagField; style: string } | null>(null)
    const edited = $derived(editing === null ? undefined : shown.find((row) => row.field === editing?.field))

    /** Gives the pane focus (Tab from the other pane). */
    export function focus(): void {
        list?.focus()
    }

    function openEditor(index: number): void {
        const row = shown.at(index)
        const cell = list?.querySelector<HTMLElement>(`[data-index="${String(index)}"] .col-value`)
        if (!editable || row === undefined || !cell || !scroller) return
        editing = { field: row.field, style: editorBox(scroller, cell, isMultiLineField(row.field) ? null : rowHeight) }
    }

    function closeEditor(refocus: boolean): void {
        editing = null
        if (refocus) list?.focus()
    }

    function commit(row: (typeof shown)[number], typed: string): void | Promise<void> {
        const edit = parseSharedEdit(row.field, row.text, typed, row.more)
        if (edit) return onEdit([edit])
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
            openEditor(at)
            return
        }
        const move = paneCursorMove(e, { count: shown.length, from: at, visibleRows: visibleRowCount(scroller, rowHeight) })
        if (move === null || move.extend) return
        claimKey(e)
        onCursorChange(move.to)
        revealRow(list, move.to)
    }
</script>

<div class="pane full-list-container" class:is-focused={focused}>
    <div class="header-row" style:grid-template-columns={GRID} aria-hidden="true">
        <span class="cut">{tString('multiRename.tags.album.colName')}</span>
        <span class="cut">{tString('multiRename.tags.album.colValue')}</span>
    </div>
    <div class="full-list" bind:this={scroller}>
        <div
            bind:this={list}
            class="listbox-region"
            role="listbox"
            tabindex="0"
            aria-label={tString('multiRename.tags.album.label')}
            aria-activedescendant={current ? `tag-album-row-${current.field}` : undefined}
            {onkeydown}
        >
            {#each shown as row, index (row.field)}
                <div
                    id="tag-album-row-{row.field}"
                    role="option"
                    tabindex="-1"
                    aria-selected={index === at}
                    aria-describedby={row.invalid ? DATE_HINT_ID : undefined}
                    class="file-entry"
                    class:is-under-cursor={index === at}
                    class:is-striped={striped && index % 2 === 1}
                    class:is-invalid={row.invalid}
                    data-index={index}
                    style:height="{rowHeight}px"
                    style:grid-template-columns={GRID}
                    onmousedown={() => { onCursorChange(index) }}
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
                value={row.text}
                ariaLabel={tString('multiRename.tags.album.editLabel', { field: row.label })}
                multiline={isMultiLineField(row.field)}
                invalid={row.invalid}
                describedBy={row.invalid ? DATE_HINT_ID : undefined}
                style={editing.style}
                onCommit={(typed: string) => commit(row, typed)}
                onClose={closeEditor}
            />
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
                aria-label={tString('multiRename.tags.clearRowLabel', { field: row.label })}
                onclick={() => { void onEdit([{ kind: 'setAll', field: row.field, values: [] }]) }}
            >
                {tString('multiRename.tags.clearRow')}
            </Button>
        </div>
    {/if}
    {#if cover}
        <!-- The cover is the pane's last group; `CoverPanel` carries its own heading. -->
        <div class="cover-group">{@render cover()}</div>
    {/if}
</div>

<style>
    /* The file panes' Full view: `FullList.svelte`'s container, header, scroller, gutter, and row box. The stripe,
       selection, and cursor fills come from the global `app-file-list.css` through the same classes. */
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

    /* Long names and values are cut with …; the tooltip shows the whole text. The block padding is FullList's
       descender room for clipped text (the flat line height would cut a "g"). */
    .cut {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        padding-block: 0.2em;
        margin-block: -0.2em;
    }

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
