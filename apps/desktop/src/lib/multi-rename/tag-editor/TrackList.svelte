<script lang="ts">
    /**
     * The audio files of the session, in the sheet's order: track number, title, length and file name. Number and
     * title edit in place (one file each), and a button plays the track. A single-file session has no track-number column:
     * a single-file book has no track number.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { TrackRow } from '#lib/ipc/bindings.js'
    import type { TagEdit } from '#lib/tauri-commands/index.js'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import EditableCell from './EditableCell.svelte'
    import PlayButton from './PlayButton.svelte'
    import { formatTrackLength, parseFieldText } from './shared-field-input'

    interface Props {
        tracks: TrackRow[]
        editable: boolean
        /** The checked tracks' paths: the tag tools work on these alone. */
        selected: ReadonlySet<string>
        onToggle: (path: string) => void
        /** The header box: check every track (`true`) or uncheck them all. */
        onToggleAll: (on: boolean) => void
        /** The path of the track playing now, or `null`. */
        playing: string | null
        /** Plays the track at `path`, or stops it when it is the one playing. */
        onPlay: (path: string) => void
        onEdit: (edits: TagEdit[]) => void | Promise<void>
    }

    const { tracks, editable, selected, onToggle, onToggleAll, playing, onPlay, onEdit }: Props = $props()

    const showNumber = $derived(tracks.length !== 1)
    const checkedCount = $derived(tracks.filter((track) => selected.has(track.path)).length)
    const allChecked = $derived(tracks.length > 0 && checkedCount === tracks.length)

    function setOne(path: string, field: 'title' | 'track', text: string): void | Promise<void> {
        return onEdit([{ kind: 'setOne', path, field, values: parseFieldText(field, text) }])
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
                <th scope="col" class="select">
                    <Checkbox
                        ariaLabel={tString('multiRename.tags.select.all')}
                        checked={allChecked}
                        indeterminate={checkedCount > 0 && !allChecked}
                        disabled={!editable || tracks.length === 0}
                        onCheckedChange={() => { onToggleAll(!allChecked) }}
                    />
                </th>
                <th scope="col" class="play"><span class="sr-only">{tString('multiRename.tags.col.play')}</span></th>
                {#if showNumber}<th scope="col" class="number">{tString('multiRename.tags.col.number')}</th>{/if}
                <th scope="col">{tString('multiRename.tags.col.title')}</th>
                <th scope="col" class="length">{tString('multiRename.tags.col.length')}</th>
                <th scope="col">{tString('multiRename.tags.col.fileName')}</th>
            </tr>
        </thead>
        <tbody>
            {#each tracks as track, index (track.path)}
                {@const editableRow = editable && track.readError === null}
                <tr class:unreadable={track.readError !== null} class:changed={track.changed} data-track-path={track.path}>
                    <td class="select">
                        <Checkbox
                            ariaLabel={tString('multiRename.tags.select.track', { fileName: track.fileName })}
                            checked={selected.has(track.path)}
                            disabled={!editable}
                            onCheckedChange={() => { onToggle(track.path) }}
                        />
                    </td>
                    <td class="play">
                        <PlayButton
                            fileName={track.fileName}
                            playing={playing === track.path}
                            disabled={track.readError !== null}
                            onToggle={() => { onPlay(track.path) }}
                        />
                    </td>
                    {#if showNumber}
                        <td class="number">
                            <EditableCell
                                id="tag-track-number-{index}"
                                value={track.track ?? ''}
                                ariaLabel={tString('multiRename.tags.track.numberLabel', { fileName: track.fileName })}
                                disabled={!editableRow}
                                onCommit={(text: string) => setOne(track.path, 'track', text)}
                            />
                        </td>
                    {/if}
                    <td>
                        <EditableCell
                            id="tag-track-title-{index}"
                            value={track.title ?? ''}
                            ariaLabel={tString('multiRename.tags.track.titleLabel', { fileName: track.fileName })}
                            disabled={!editableRow}
                            onCommit={(text: string) => setOne(track.path, 'title', text)}
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

    .select {
        width: 32px;
    }

    .play {
        width: 36px;
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
