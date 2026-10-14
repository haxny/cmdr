<script lang="ts">
    /**
     * A queue row's first line after its icon: what it's doing, to what, and where.
     * "Copying 2 folders and 1 file Dropbox → cmdr-s3-test/target/folder" on every
     * row whose starter named its items (`snapshot.items`, `./row-items.ts`); a
     * rename, a new folder, or a reversal keeps the two short names it had.
     * DETAILS § "Row layout".
     */
    import type { OperationSnapshot } from '#lib/ipc/bindings.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { useShortenMiddle } from '#lib/utils/shorten-middle-action.js'
    import type { RollbackConfirmVariant } from '../reversal-wording'
    import { itemsPhrase, itemsTooltip, locationName } from './row-items'

    interface Props {
        /** The row's `aria-labelledby` points here, with its status. */
        id: string
        /** The action word ("Copying"), already picked by the row. */
        label: string
        snapshot: OperationSnapshot
        /** Set when this operation IS a reversal, which words its lone folder. */
        reversalVariant: RollbackConfirmVariant | null
    }

    const { id, label, snapshot, reversalVariant }: Props = $props()

    const items = $derived(snapshot.items)
    const itemsText = $derived(items === null ? '' : itemsPhrase(items))
    const itemsHover = $derived(items === null ? '' : itemsTooltip(items))
    const fromName = $derived(items?.from == null ? '' : locationName(items.from))

    // Without items: source / destination basenames, the full paths in the tooltip.
    // Delete / trash have no destination.
    function basename(path: string | null): string {
        if (!path) return ''
        const trimmed = path.replace(/\/+$/, '')
        const idx = trimmed.lastIndexOf('/')
        return idx >= 0 ? trimmed.slice(idx + 1) : trimmed
    }
    const sourceName = $derived(basename(snapshot.source))
    const destName = $derived(basename(snapshot.destination))

    /** A removal reversal has no destination, so the arrow that gives every other
     *  row its direction never renders and the lone folder name sits against
     *  "Deleting what it created" reading as the thing about to go. The preposition
     *  is what puts it back where it belongs: the reversal clears files INSIDE that
     *  folder. Only this shape needs it; every other row already has its arrow. */
    const sourcePhrase = $derived(
        reversalVariant === 'undoByDeleting' && snapshot.destination === null
            ? tString('queue.row.reversalInFolder', { folder: sourceName })
            : sourceName,
    )
</script>

<div class="summary-row queue-summary" role="gridcell" {id}>
    <span class="op-label">{label}</span>
    {#if items}
        <!-- Always on display, so two copies to different places can be told
             apart at a glance. The names behind the count are on hover. -->
        <span class="items" use:tooltip={itemsHover}>{itemsText}</span>
        {#if items.from !== null}
            <!-- A delete has no arrow to say which way, so its folder takes "in". -->
            <span class="path from" use:tooltip={items.from}
                >{items.to === null ? tString('queue.row.itemsInFolder', { folder: fromName }) : fromName}</span
            >
        {/if}
        {#if items.to !== null}
            <span class="arrow" aria-hidden="true">&#x2192;</span>
            <!-- Elided in the MIDDLE, so both the drive and the last folder stay
                 readable; the full path is on hover, and for a screen reader. -->
            <span
                class="path dest"
                aria-hidden="true"
                use:useShortenMiddle={{ text: items.to, preferBreakAt: '/', tooltip: items.to }}
            ></span>
            <span class="sr-only">{items.to}</span>
        {/if}
    {:else}
        {#if snapshot.source}
            <span class="path" use:tooltip={{ text: snapshot.source, overflowOnly: true }}>{sourcePhrase}</span>
        {/if}
        {#if snapshot.destination}
            <span class="arrow" aria-hidden="true">&#x2192;</span>
            <span class="path dest" use:tooltip={{ text: snapshot.destination, overflowOnly: true }}>{destName}</span>
        {/if}
    {/if}
</div>

<style>
    .summary-row {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
        min-width: 0;
        font-size: var(--font-size-sm);
    }

    .op-label {
        font-weight: 500;
        color: var(--color-text-primary);
        flex-shrink: 0;
    }

    /* What it acts on never gives way: it's what tells two rows apart. */
    .items {
        flex-shrink: 0;
        white-space: nowrap;
        color: var(--color-text-primary);
    }

    .path {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-secondary);
        min-width: 0;
    }

    /* The source folder gives way before the destination does, and never takes
       more than its share: the destination is elided in the middle instead. */
    .path.from {
        flex: 0 1 auto;
        max-width: 40%;
    }

    .path.dest {
        flex: 1 1 auto;
        color: var(--color-accent-text);
    }

    .arrow {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }
</style>
