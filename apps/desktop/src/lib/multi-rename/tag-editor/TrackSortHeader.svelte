<script lang="ts">
    /**
     * One sortable column header of the Tracks pane: `file-explorer/selection/SortableHeader.svelte`'s button and
     * caret, keyed by the pane's `TrackSortKey` instead of a file column. A native button, so Tab reaches it and
     * Enter / Space click it.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { TrackSort, TrackSortKey } from '#lib/ipc/bindings.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'

    interface Props {
        sortKey: TrackSortKey
        label: string
        /** The order the tracks are in, or `null` when no header's order is (Multi-Rename moved a row). */
        current: TrackSort | null
        onClick: (key: TrackSortKey) => void
        /** 'right' for numeric columns: the caret then sits left of the label, so the numbers' edge stays clean. */
        align?: 'left' | 'right'
        /** The column's `data-col`, the one its row cells carry, so the header cell and its column are paired. */
        col?: string
    }

    const { sortKey, label, current, onClick, align = 'left', col }: Props = $props()

    const isActive = $derived(current?.key === sortKey)
    const order = $derived(!isActive ? 'none' : current?.descending === true ? 'descending' : 'ascending')
    // The spoken name starts with the visible label (WCAG 2.5.3) and carries the state, as file panes' headers do.
    const ariaLabel = $derived(tString('multiRename.tags.sort.state', { column: label, order }))
</script>

<button
    class="sortable-header"
    class:is-active={isActive}
    class:align-right={align === 'right'}
    data-col={col}
    onclick={() => { onClick(sortKey) }}
    type="button"
    aria-label={ariaLabel}
    use:tooltip={{ text: tString('multiRename.tags.sort.by', { column: label }) }}
>
    <span class="label">{label}</span>
    <span class="sort-indicator" hidden={!isActive} aria-hidden="true">
        {order === 'descending' ? '▼' : '▲'}
    </span>
</button>

<style>
    /* Same look as the file panes' headers. The negative margin pulls the button 4px outside its column track, so
       with its own 4px padding the label still lines up with the cells below and the hover fill gets some room. */
    .sortable-header {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: var(--spacing-xs);
        height: 100%;
        min-width: 0;
        margin-block: 0;
        margin-inline: calc(-1 * var(--spacing-xs));
        padding-block: 0;
        padding-inline: var(--spacing-xs);
        border: 0;
        border-radius: var(--radius-sm);
        background-color: transparent;
        color: var(--color-text-secondary);
        font: inherit;
        font-size: var(--font-size-sm);
        text-align: start;
        white-space: nowrap;
        transition:
            color var(--transition-fast),
            background-color var(--transition-fast);
    }

    .sortable-header:hover {
        background-color: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .sortable-header.is-active {
        color: var(--color-accent-text);
        font-weight: 500;
    }

    /* `row-reverse` puts the label at the right edge and the caret to its left. */
    .align-right {
        flex-direction: row-reverse;
    }

    .label {
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .sort-indicator {
        flex-shrink: 0;
        font-size: 8px;
    }
</style>
