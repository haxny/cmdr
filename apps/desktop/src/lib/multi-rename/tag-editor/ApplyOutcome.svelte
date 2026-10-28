<script lang="ts">
    /**
     * What a tag write did: how many files got their tags, and every file it left alone, each in its own list:
     * failed (with why, by the error's kind), changed on disk since it was read (with a way to read the files again),
     * and skipped by the user; the side files beside the book (`reader.txt`, `desc.txt`) it wrote, and those it
     * left as they were; when no file was written, it says that first. A write whose old tags couldn't be kept says it can't be undone. Nothing is left out.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { formatUnitList } from '#lib/intl/list-format.js'
    import Button from '#lib/ui/Button.svelte'
    import { failureKey, failureParams, fileNameOf, type ApplyOutcome } from './apply-outcome'

    interface Props {
        outcome: ApplyOutcome
        /** Undo can still reverse some files of this write (the ones whose old tags were kept). */
        someUndoable: boolean
        /** Reads every file again (after "changed on disk"). */
        onReload: () => void
        /** How a file is named in the lists (its path below the book folder); the bare file name by default. */
        nameOf?: (path: string) => string
    }

    const { outcome, someUndoable, onReload, nameOf = fileNameOf }: Props = $props()

    const lists = $derived(
        [
            {
                heading: 'multiRename.tags.write.failedHeading' as MessageKey,
                rows: outcome.failed.map((row) =>
                    tString('multiRename.tags.write.failedRow', {
                        fileName: nameOf(row.path),
                        reason: tString(failureKey(row.error), failureParams(row.error)),
                    }),
                ),
            },
            {
                heading: 'multiRename.tags.write.changedOnDiskHeading' as MessageKey,
                rows: outcome.changedOnDisk.map(nameOf),
            },
            {
                heading: 'multiRename.tags.write.skippedHeading' as MessageKey,
                rows: outcome.skipped.map((row) => nameOf(row.path)),
            },
            {
                heading: 'multiRename.tags.write.sideNotWrittenHeading' as MessageKey,
                rows: outcome.sideFilesNotWritten.map(fileNameOf),
            },
        ].filter((list) => list.rows.length > 0),
    )
</script>

<div class="outcome">
    {#if outcome.written + outcome.sideFilesWritten.length === 0 && lists.length > 0}
        <!-- An alert only when files were refused; files the user chose to skip are no problem to announce. -->
        <p role={outcome.failed.length + outcome.changedOnDisk.length > 0 ? 'alert' : 'status'}>
            {tString('multiRename.tags.write.refused')}
        </p>
    {/if}
    {#if outcome.written > 0}
        <p role="status">
            {tString('multiRename.tags.write.done', { count: outcome.written, countText: formatInteger(outcome.written) })}
        </p>
    {/if}
    {#if outcome.sideFilesWritten.length > 0}
        <p role="status">
            {tString('multiRename.tags.write.sideWritten', { files: formatUnitList(outcome.sideFilesWritten.map(fileNameOf)) })}
        </p>
    {/if}
    {#if outcome.undoUnavailable}
        <p class="note">
            {tString(someUndoable ? 'multiRename.tags.write.noUndoSome' : 'multiRename.tags.write.noUndo')}
        </p>
    {/if}
    {#each lists as list (list.heading)}
        <p>{tString(list.heading)}</p>
        <ul aria-label={tString(list.heading)}>
            {#each list.rows as row, index (index)}
                <li>{row}</li>
            {/each}
        </ul>
    {/each}
    {#if outcome.changedOnDisk.length > 0}
        <div>
            <Button size="mini" onclick={onReload}>{tString('multiRename.tags.write.reload')}</Button>
        </div>
    {/if}
</div>

<style>
    .outcome {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    p {
        margin: 0;
    }

    .note {
        color: var(--color-warning-text);
    }

    ul {
        margin: 0;
        padding-left: var(--spacing-lg);
        max-height: 120px;
        overflow: auto;
        overflow-wrap: anywhere;
    }
</style>
