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
    import { formatFieldValue, isMultiLineField, parseFieldText } from './shared-field-input'

    interface Props {
        rows: SharedRow[]
        editable: boolean
        onEdit: (edits: TagEdit[]) => void | Promise<void>
    }

    const { rows, editable, onEdit }: Props = $props()

    const shown = $derived(
        rows.map((row) => ({ field: row.field, choices: row.choices, empty: row.value === 'empty', ...formatFieldValue(row.value) })),
    )
</script>

<div class="shared" role="group" aria-label={tString('multiRename.tags.shared.label')}>
    {#each shown as row (row.field)}
        <label class="label" class:top={isMultiLineField(row.field)} for="tag-field-{row.field}">
            {tString(FIELD_LABELS[row.field])}
        </label>
        <EditableCell
            id="tag-field-{row.field}"
            value={row.text}
            placeholder={row.multiple ? tString('multiRename.tags.multipleValues') : undefined}
            multiline={isMultiLineField(row.field)}
            disabled={!editable}
            onCommit={(text: string) => onEdit([{ kind: 'setAll', field: row.field, values: parseFieldText(row.field, text) }])}
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

    .label.top {
        align-self: start;
        padding-top: var(--spacing-xs);
    }
</style>
