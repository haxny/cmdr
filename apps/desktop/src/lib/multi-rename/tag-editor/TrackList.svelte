<script lang="ts">
    /**
     * The audio files of the session, in the sheet's order: track number, title, length and file name. Number and
     * title edit in place (one file each). A single-file session has no track-number column: a single-file book has
     * no track number.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { TrackRow } from '#lib/ipc/bindings.js'
    import type { TagEdit } from '#lib/tauri-commands/index.js'
    import EditableCell from './EditableCell.svelte'
    import { formatTrackLength, parseFieldText } from './shared-field-input'

    interface Props {
        tracks: TrackRow[]
        editable: boolean
        onEdit: (edits: TagEdit[]) => void
    }

    const { tracks, editable, onEdit }: Props = $props()

    const showNumber = $derived(tracks.length !== 1)

    function setOne(path: string, field: 'title' | 'track', text: string): void {
        onEdit([{ kind: 'setOne', path, field, values: parseFieldText(field, text) }])
    }
</script>

<div class="scroll">
    <table class="tracks" aria-label={tString('multiRename.tags.tracks.label')}>
        <caption class="count">
            {tString('multiRename.tags.trackCount', {
                count: tracks.length,
                countText: formatInteger(tracks.length),
            })}
        </caption>
        <thead>
            <tr>
                {#if showNumber}<th scope="col" class="number">{tString('multiRename.tags.col.number')}</th>{/if}
                <th scope="col">{tString('multiRename.tags.col.title')}</th>
                <th scope="col" class="length">{tString('multiRename.tags.col.length')}</th>
                <th scope="col">{tString('multiRename.tags.col.fileName')}</th>
            </tr>
        </thead>
        <tbody>
            {#each tracks as track, index (track.path)}
                {@const editableRow = editable && track.readError === null}
                <tr class:unreadable={track.readError !== null} class:changed={track.changed}>
                    {#if showNumber}
                        <td class="number">
                            <EditableCell
                                id="tag-track-number-{index}"
                                value={track.track ?? ''}
                                ariaLabel={tString('multiRename.tags.track.numberLabel', { fileName: track.fileName })}
                                disabled={!editableRow}
                                onCommit={(text) => { setOne(track.path, 'track', text) }}
                            />
                        </td>
                    {/if}
                    <td>
                        <EditableCell
                            id="tag-track-title-{index}"
                            value={track.title ?? ''}
                            ariaLabel={tString('multiRename.tags.track.titleLabel', { fileName: track.fileName })}
                            disabled={!editableRow}
                            onCommit={(text) => { setOne(track.path, 'title', text) }}
                        />
                    </td>
                    <td class="length">{formatTrackLength(track.durationMs)}</td>
                    <td class="file">
                        {track.fileName}
                        {#if track.readError !== null}
                            <span class="note">{tString('multiRename.tags.track.unreadable')}</span>
                        {/if}
                    </td>
                </tr>
            {/each}
        </tbody>
    </table>
</div>

<style>
    .scroll {
        overflow: auto;
        min-height: 0;
        flex: 1;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
    }

    .tracks {
        width: 100%;
        border-collapse: collapse;
    }

    .count {
        padding: var(--spacing-xs) var(--spacing-sm);
        color: var(--color-text-secondary);
        text-align: left;
    }

    th {
        position: sticky;
        top: 0;
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-weight: normal;
        text-align: left;
    }

    td {
        padding: var(--spacing-xxs) var(--spacing-sm);
    }

    .number {
        width: 72px;
    }

    .length {
        width: 80px;
        text-align: right;
        font-variant-numeric: tabular-nums;
    }

    .file {
        color: var(--color-text-secondary);
        overflow-wrap: break-word;
    }

    tr.unreadable,
    tr.unreadable .file {
        color: var(--color-text-tertiary);
    }

    tr.changed .file {
        color: var(--color-text-primary);
    }

    .note {
        display: block;
        font-size: var(--font-size-xs);
    }
</style>
