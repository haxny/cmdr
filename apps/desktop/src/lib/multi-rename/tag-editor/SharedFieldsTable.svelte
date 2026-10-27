<script lang="ts">
    /**
     * The tag fields the files share, one row each (Title and track number live in the track list). A row whose
     * files differ shows «multiple values» and sends nothing until the user types; typing sets the field on every file.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { SharedRow, TagField } from '#lib/ipc/bindings.js'
    import type { TagEdit } from '#lib/tauri-commands/index.js'
    import EditableCell from './EditableCell.svelte'
    import { formatFieldValue, isMultiLineField, parseFieldText } from './shared-field-input'

    interface Props {
        rows: SharedRow[]
        editable: boolean
        onEdit: (edits: TagEdit[]) => void
    }

    const { rows, editable, onEdit }: Props = $props()

    const LABELS: Readonly<Record<TagField, MessageKey>> = {
        title: 'multiRename.tags.field.title',
        artist: 'multiRename.tags.field.artist',
        albumArtist: 'multiRename.tags.field.albumArtist',
        album: 'multiRename.tags.field.album',
        performer: 'multiRename.tags.field.performer',
        composer: 'multiRename.tags.field.composer',
        date: 'multiRename.tags.field.date',
        genre: 'multiRename.tags.field.genre',
        publisher: 'multiRename.tags.field.publisher',
        translator: 'multiRename.tags.field.translator',
        track: 'multiRename.tags.field.track',
        disc: 'multiRename.tags.field.disc',
        comment: 'multiRename.tags.field.comment',
        description: 'multiRename.tags.field.description',
        www: 'multiRename.tags.field.www',
    }

    const shown = $derived(rows.map((row) => ({ field: row.field, ...formatFieldValue(row.value) })))
</script>

<div class="shared" role="group" aria-label={tString('multiRename.tags.shared.label')}>
    {#each shown as row (row.field)}
        <label class="label" class:top={isMultiLineField(row.field)} for="tag-field-{row.field}">
            {tString(LABELS[row.field])}
        </label>
        <EditableCell
            id="tag-field-{row.field}"
            value={row.text}
            placeholder={row.multiple ? tString('multiRename.tags.multipleValues') : undefined}
            multiline={isMultiLineField(row.field)}
            disabled={!editable}
            onCommit={(text) => {
                onEdit([{ kind: 'setAll', field: row.field, values: parseFieldText(row.field, text) }])
            }}
        />
    {/each}
</div>

<style>
    .shared {
        display: grid;
        grid-template-columns: max-content minmax(0, 1fr);
        gap: var(--spacing-xs) var(--spacing-md);
        align-items: center;
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
