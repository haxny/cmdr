<!--
    One row of the transfer tree: chevron, icon, the middle-truncated label (its
    full relative path on hover), the count line, a small bar while it's at work,
    and a mark once it settled. Everything it says comes from `row-format.ts`.

    It fills the slot the tree places at its flat index. The tree owns the
    keyboard and the clicks (delegated); the chevron only carries a
    `data-chevron` hook, and its indent comes inline from its depth.

    `pinned` draws the same row as a sticky header: a mirror of a real row, so
    it carries no tree semantics and no tab stop (the tree's stack is
    `aria-hidden`; `DETAILS.md` § "Pinned headers").
-->
<script lang="ts">
    import type { ProgressAxis, TransferTreeRow } from '#lib/tauri-commands/index.js'
    import Icon from '#lib/ui/Icon.svelte'
    import ProgressBar from '#lib/ui/ProgressBar.svelte'
    import StatusGlyph from '#lib/ui/StatusGlyph.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { useShortenMiddle } from '#lib/utils/shorten-middle-action.js'
    import { accessibleName, barFraction, countSegments, stateWord, type TransferTreeOperation } from './row-format'

    interface Props {
        row: TransferTreeRow
        /** Its flat index under the current expansion. */
        index: number
        axis: ProgressAxis
        operation: TransferTreeOperation
        /** Its path relative to the transfer's root, for the hover. */
        path: string
        /** The roving tab stop: the one row Tab lands on. */
        tabStop: boolean
        /** A sticky header mirroring the row at `index`, not a treeitem. */
        pinned?: boolean
    }

    const { row, index, axis, operation, path, tabStop, pinned = false }: Props = $props()

    const isFolder = $derived(row.kind === 'folder')
    const segments = $derived(countSegments(row, operation))
    const name = $derived(accessibleName(row, operation))
    const atWork = $derived(row.state === 'active')
    const barLabel = $derived(
        isFolder && axis === 'files'
            ? tString('fileOperations.transferProgress.fileProgressAria')
            : tString('fileOperations.transferProgress.sizeProgressAria'),
    )
</script>

<div
    class="tree-row"
    class:at-work={atWork}
    role={pinned ? undefined : 'treeitem'}
    aria-level={pinned ? undefined : row.depth + 1}
    aria-posinset={pinned ? undefined : row.posInSet}
    aria-setsize={pinned ? undefined : row.setSize}
    aria-expanded={!pinned && isFolder && row.hasChildren ? row.expanded : undefined}
    aria-label={pinned ? undefined : name}
    aria-selected={pinned ? undefined : false}
    tabindex={pinned ? undefined : tabStop ? 0 : -1}
    data-index={pinned ? undefined : index}
    data-pinned-index={pinned ? index : undefined}
    data-row-id={row.id}
    data-state={row.state}
    style:padding-inline-start="calc(var(--spacing-xs) + {row.depth} * var(--spacing-lg))"
>
    <span class="chevron" class:open={row.expanded} data-chevron={isFolder && row.hasChildren ? '' : undefined}>
        {#if isFolder && row.hasChildren}<Icon name="chevron-right" size={12} aria-hidden="true" />{/if}
    </span>
    <span class="kind-icon" aria-hidden="true">
        <Icon name={isFolder ? (row.expanded ? 'folder-open' : 'folder') : 'file'} size={14} />
    </span>
    <span
        class="label"
        use:useShortenMiddle={{
            text: row.label,
            preferBreakAt: '/',
            tooltip: path === row.label ? undefined : path,
            tooltipWhenTruncated: true,
        }}
    ></span>
    <span class="count">
        {#each segments as segment, i (i)}
            {#if i > 0}<span class="separator" aria-hidden="true">·</span>{/if}
            <span class="segment" class:error={segment.tone === 'error'}>{segment.text}</span>
        {/each}
    </span>
    <span class="bar">
        {#if atWork}<ProgressBar value={barFraction(row, axis)} size="sm" animated={false} ariaLabel={barLabel} />{/if}
    </span>
    <span class="mark">
        {#if row.state === 'done'}
            <span class="mark-done"><StatusGlyph name="check" label={stateWord('done', operation)} tooltip={null} /></span>
        {:else if row.state === 'skipped'}
            <StatusGlyph name="circle-slash" label={stateWord('skipped', operation)} tooltip={null} />
        {:else if row.state === 'failed'}
            <span class="mark-failed"
                ><StatusGlyph name="circle-x" label={stateWord('failed', operation)} tooltip={null} /></span
            >
        {/if}
    </span>
</div>

<style>
    .tree-row {
        box-sizing: border-box;
        height: 100%;
        display: grid;
        grid-template-columns: 12px 14px minmax(0, 1fr) auto 56px 14px;
        align-items: center;
        column-gap: var(--spacing-xs);
        padding-inline-end: var(--spacing-sm);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        cursor: default;
        user-select: none;
    }

    .tree-row:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .at-work {
        color: var(--color-text-primary);
    }

    .chevron,
    .kind-icon,
    .mark {
        display: inline-flex;
        align-items: center;
        justify-content: center;
    }

    .chevron {
        color: var(--color-text-tertiary);
    }

    @media (prefers-reduced-motion: no-preference) {
        .chevron :global(svg) {
            transition: transform 0.12s ease-out;
        }
    }

    .chevron.open :global(svg) {
        transform: rotate(90deg);
    }

    .kind-icon {
        color: var(--color-text-tertiary);
    }

    .label {
        min-width: 0;
    }

    .count {
        white-space: nowrap;
        color: var(--color-text-tertiary);
        font-variant-numeric: tabular-nums;
        text-align: end;
    }

    .separator {
        margin-inline: var(--spacing-xs);
    }

    .segment.error {
        color: var(--color-error-text);
    }

    .bar {
        display: flex;
        align-items: center;
    }

    .mark {
        color: var(--color-text-tertiary);
    }

    .mark-done {
        display: inline-flex;
        color: var(--color-allow);
    }

    .mark-failed {
        display: inline-flex;
        color: var(--color-error-text);
    }

    /* StatusGlyph's leading gap is for a glyph after text; this column is its own cell. */
    .mark :global(.status-glyph) {
        margin-left: 0;
    }
</style>
