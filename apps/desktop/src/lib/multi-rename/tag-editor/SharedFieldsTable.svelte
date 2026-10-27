<script lang="ts">
    /**
     * The tag fields the files share, one row each (Title and track number live in the track list). A row whose
     * files differ shows «multiple values» and sends nothing until the user types; typing sets the field on every file.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { SharedRow } from '#lib/ipc/bindings.js'
    import type { TagEdit } from '#lib/tauri-commands/index.js'
    import Button from '#lib/ui/Button.svelte'
    import EditableCell from './EditableCell.svelte'
    import { FIELD_LABELS } from './field-labels'
    import ValueChoices from './ValueChoices.svelte'
    import { formatFieldValue, isMultiLineField, multipleText, parseSharedEdit } from './shared-field-input'

    interface Props {
        rows: SharedRow[]
        editable: boolean
        /** How many files carry a Date the writer would refuse; the Date row is flagged while it is above 0. */
        badDates?: number
        onEdit: (edits: TagEdit[]) => void | Promise<void>
    }

    const { rows, editable, badDates = 0, onEdit }: Props = $props()

    const shown = $derived(
        rows.map((row) => {
            const { text, multiple } = formatFieldValue(row.value)
            return {
                field: row.field,
                choices: row.choices,
                empty: row.value === 'empty',
                multiple,
                text: multiple ? multipleText(row.choices) : text,
            }
        }),
    )

    function commit(row: (typeof shown)[number], typed: string): void | Promise<void> {
        const edit = parseSharedEdit(row.field, row.text, typed)
        if (edit) return onEdit([edit])
    }
</script>

<div class="shared" role="group" aria-label={tString('multiRename.tags.shared.label')}>
    {#each shown as row (row.field)}
        {@const badDate = row.field === 'date' && badDates > 0}
        <label class="label" class:top={isMultiLineField(row.field)} class:bad={badDate} for="tag-field-{row.field}">
            {tString(FIELD_LABELS[row.field])}
        </label>
        <EditableCell
            id="tag-field-{row.field}"
            value={row.text}
            multiline={isMultiLineField(row.field)}
            disabled={!editable}
            invalid={badDate}
            describedBy={badDate ? 'tag-field-date-hint' : undefined}
            onCommit={(typed: string) => commit(row, typed)}
        />
        <span class="actions">
            {#if row.multiple && row.choices.length > 0}
                <ValueChoices
                    field={tString(FIELD_LABELS[row.field])}
                    choices={row.choices}
                    disabled={!editable}
                    onPick={(values: string[]) => { void onEdit([{ kind: 'setAll', field: row.field, values }]) }}
                />
            {/if}
            <Button
                size="mini"
                disabled={!editable || row.empty}
                aria-label={tString('multiRename.tags.clearRowLabel', { field: tString(FIELD_LABELS[row.field]) })}
                onclick={() => { void onEdit([{ kind: 'setAll', field: row.field, values: [] }]) }}
            >
                {tString('multiRename.tags.clearRow')}
            </Button>
        </span>
        {#if badDate}
            <p class="hint" id="tag-field-date-hint">{tString('multiRename.tags.date.invalid')}</p>
        {/if}
    {/each}
</div>

<style>
    .shared {
        display: grid;
        grid-template-columns: max-content minmax(0, 1fr) max-content;
        gap: var(--spacing-xs) var(--spacing-md);
        align-items: center;
    }

    .actions {
        display: flex;
        gap: var(--spacing-xs);
    }

    .label {
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .label.bad,
    .hint {
        color: var(--color-error-text);
    }

    .hint {
        grid-column: 2 / -1;
        margin: 0;
        font-size: var(--font-size-sm);
    }

    /* A long value is cut with … by the display only; the box keeps the whole text. */
    .shared :global(input) {
        text-overflow: ellipsis;
    }

    .label.top {
        align-self: start;
        padding-top: var(--spacing-xs);
    }
</style>
