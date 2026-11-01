<script lang="ts">
    /**
     * The Synchronize folders grid: TC's columns (left name, size, date, action, date, size, right name), one row
     * per pair under folder header rows, paged from the backend. Each row's accessible name says in words what its
     * arrow, marks, and colour show (`row-label.ts`). The grid is the focus stop; its keys go to the dialog
     * (`onKeydown`), which owns what they do.
     */
    import ColumnList from '#lib/ui/ColumnList.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import type { IconName } from '#lib/ui/icons/icon-map.js'
    import {
        columnListProps,
        type ColumnListApi,
        type ColumnListColumn,
        type ColumnListWindowedSource,
    } from '#lib/ui/column-list-types.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { formatByteSize } from '#lib/units/index.js'
    import { formatDateTime } from '#lib/settings/reactive-settings.svelte.js'
    import type { Category, ViewRowWire } from '#lib/tauri-commands/index.js'
    import {
        ACTION_GLYPH,
        RUNG_BADGE,
        actionLabel,
        categoryLabel,
        isChangedByYou,
        rowAccessibleName,
        rungLabel,
        type SyncItemRow,
    } from './row-label'
    import type { SyncSession } from './sync-session.svelte'

    interface Props {
        session: SyncSession
        cursorIndex: number
        onCursorChange: (index: number) => void
        onKeydown: (event: KeyboardEvent) => void
        onRowClick: (index: number, event: MouseEvent) => void
        onRowDoubleClick: (index: number) => void
        onRowContextMenu: (payload: { index: number; event: MouseEvent }) => void
    }

    const { session, cursorIndex, onCursorChange, onKeydown, onRowClick, onRowDoubleClick, onRowContextMenu }: Props =
        $props()

    let list: ColumnListApi | undefined = $state()
    let containerEl: HTMLDivElement | undefined = $state()

    /** TC's category marks as shapes, so a category never rests on colour (spec 9). */
    const CATEGORY_ICON: Record<Category, IconName> = {
        leftOnly: 'circle-dot',
        rightOnly: 'circle-dot',
        newerLeft: 'clock',
        newerRight: 'clock',
        different: 'circle-alert',
        equal: 'circle-check',
        unknown: 'circle-dashed',
        typeClash: 'circle-x',
        nameClash: 'circle-slash',
        unreadable: 'lock',
    }

    const source: ColumnListWindowedSource<ViewRowWire> = {
        get count() {
            return session.total
        },
        getRow: (index) => session.getRow(index),
        onRangeChange: ({ start, end }) => {
            session.ensureRange(start, end)
        },
    }

    const item = (row: ViewRowWire): SyncItemRow | null => (row.type === 'item' ? row : null)
    const sizeText = (size: number | null): string => (size === null ? '' : formatByteSize(size))
    const dateText = (mtime: number | null): string => (mtime === null ? '' : formatDateTime(mtime))

    function rowLabel(row: ViewRowWire): string {
        const it = item(row)
        if (!it) return ''
        const name = rowAccessibleName(it)
        return session.selected.has(it.index) ? `${name}, ${tString('sync.grid.spoken.selected')}` : name
    }

    export function focus(): void {
        containerEl?.querySelector<HTMLElement>('[role="grid"]')?.focus()
    }

    export function scrollIndexIntoView(index: number): void {
        list?.scrollIndexIntoView(index)
    }

    export function cursorTarget(event: KeyboardEvent, from: number): number | null {
        return list?.cursorTarget(event, from) ?? null
    }
</script>

{#snippet markCell({ row }: { row: ViewRowWire })}
    {@const it = item(row)}
    {#if it && session.selected.has(it.index)}<span class="mark" aria-hidden="true">✓</span>{/if}
{/snippet}

{#snippet leftNameCell({ row }: { row: ViewRowWire })}
    {@const it = item(row)}
    {#if it?.leftName}
        <span class="name">
            {#if it.leftKind === 'folder'}<Icon name="folder" size={12} />{/if}{it.leftName}
        </span>
    {/if}
{/snippet}

{#snippet rightNameCell({ row }: { row: ViewRowWire })}
    {@const it = item(row)}
    {#if it?.rightName}
        <span class="name">
            {#if it.rightKind === 'folder'}<Icon name="folder" size={12} />{/if}{it.rightName}
        </span>
    {/if}
{/snippet}

{#snippet leftSizeCell({ row }: { row: ViewRowWire })}{sizeText(item(row)?.leftSize ?? null)}{/snippet}
{#snippet rightSizeCell({ row }: { row: ViewRowWire })}{sizeText(item(row)?.rightSize ?? null)}{/snippet}
{#snippet leftDateCell({ row }: { row: ViewRowWire })}{dateText(item(row)?.leftMtime ?? null)}{/snippet}
{#snippet rightDateCell({ row }: { row: ViewRowWire })}{dateText(item(row)?.rightMtime ?? null)}{/snippet}

{#snippet actionCell({ row }: { row: ViewRowWire })}
    {@const it = item(row)}
    {#if it}
        <span class="action-cell">
            <span class="category" use:tooltip={categoryLabel(it.category)} aria-hidden="true">
                <Icon name={CATEGORY_ICON[it.category]} size={12} />
            </span>
            <span
                class="glyph"
                class:to-right={it.action === 'copyRight'}
                class:to-left={it.action === 'copyLeft' || it.action === 'removeRight' || it.action === 'removeLeft'}
                class:undecided={it.action === 'undecided'} use:tooltip={actionLabel(it.action)} aria-hidden="true"
                >{ACTION_GLYPH[it.action]}</span
            >
            {#if isChangedByYou(it)}
                <span class="changed" use:tooltip={tString('sync.grid.changedMark')} aria-hidden="true">•</span>
            {/if}
            {#if it.olderOverwritesNewer}
                <span class="warning" use:tooltip={tString('sync.grid.warningMark')} aria-hidden="true">
                    <Icon name="triangle-alert" size={12} />
                </span>
            {/if}
            {#if it.content === 'pending'}
                <span class="pending" use:tooltip={tString('sync.grid.pending')} aria-hidden="true">…</span>
            {/if}
            {#if RUNG_BADGE[it.rung]}
                <span class="rung" use:tooltip={rungLabel(it.rung)}
                    ><span aria-hidden="true">{RUNG_BADGE[it.rung]}</span><span class="sr-only"
                        >{rungLabel(it.rung)}</span
                    ></span
                >
            {/if}
        </span>
    {/if}
{/snippet}

{#snippet folderHeading({ row }: { row: ViewRowWire })}
    {#if row.type === 'folder'}
        <span class="folder-heading">
            <Icon name="folder" size={12} />
            {tString('sync.grid.folderHeading', {
                path: row.relDir,
                ordinalText: formatNumber(row.ordinal),
                ofText: formatNumber(row.of),
            })}
        </span>
    {/if}
{/snippet}

<!-- Header labels: the visible word is short ("Size"), the spoken one says which side. -->
{#snippet leftSizeHeader()}<span aria-hidden="true">{tString('sync.grid.col.size')}</span><span class="sr-only"
        >{tString('sync.grid.col.leftSize')}</span
    >{/snippet}
{#snippet rightSizeHeader()}<span aria-hidden="true">{tString('sync.grid.col.size')}</span><span class="sr-only"
        >{tString('sync.grid.col.rightSize')}</span
    >{/snippet}
{#snippet leftDateHeader()}<span aria-hidden="true">{tString('sync.grid.col.date')}</span><span class="sr-only"
        >{tString('sync.grid.col.leftDate')}</span
    >{/snippet}
{#snippet rightDateHeader()}<span aria-hidden="true">{tString('sync.grid.col.date')}</span><span class="sr-only"
        >{tString('sync.grid.col.rightDate')}</span
    >{/snippet}

<div class="sync-grid" bind:this={containerEl}>
    <ColumnList
        bind:this={list}
        {...columnListProps<ViewRowWire>({
            columns: [
                {
                    id: 'mark',
                    label: tString('sync.grid.col.mark'),
                    labelHidden: true,
                    width: { kind: 'fixed', ch: 2 },
                    cell: markCell,
                },
                {
                    id: 'leftName',
                    label: tString('sync.grid.col.leftName'),
                    width: { kind: 'share', minPx: 120 },
                    emphasis: true,
                    cell: leftNameCell,
                },
                {
                    id: 'leftSize',
                    label: tString('sync.grid.col.leftSize'),
                    header: leftSizeHeader,
                    width: { kind: 'fixed', ch: 9 },
                    align: 'end',
                    tone: 'secondary',
                    cell: leftSizeCell,
                },
                {
                    id: 'leftDate',
                    label: tString('sync.grid.col.leftDate'),
                    header: leftDateHeader,
                    width: { kind: 'fixed', ch: 16 },
                    tone: 'secondary',
                    cell: leftDateCell,
                },
                {
                    id: 'action',
                    label: tString('sync.grid.col.action'),
                    width: { kind: 'fixed', ch: 10 },
                    clip: false,
                    cell: actionCell,
                },
                {
                    id: 'rightDate',
                    label: tString('sync.grid.col.rightDate'),
                    header: rightDateHeader,
                    width: { kind: 'fixed', ch: 16 },
                    tone: 'secondary',
                    cell: rightDateCell,
                },
                {
                    id: 'rightSize',
                    label: tString('sync.grid.col.rightSize'),
                    header: rightSizeHeader,
                    width: { kind: 'fixed', ch: 9 },
                    align: 'end',
                    tone: 'secondary',
                    cell: rightSizeCell,
                },
                {
                    id: 'rightName',
                    label: tString('sync.grid.col.rightName'),
                    width: { kind: 'share', minPx: 120 },
                    emphasis: true,
                    cell: rightNameCell,
                },
            ] satisfies ColumnListColumn<ViewRowWire>[],
            rows: source,
            semantics: 'grid',
            ariaLabel: tString('sync.grid.label'),
            cursorIndex,
            onCursorChange,
            onKeydown,
            onRowClick,
            onRowDoubleClick,
            onRowContextMenu: ({ index, event }) => {
                onRowContextMenu({ index, event })
            },
            rowLabel,
            isGroupHeading: (row) => row.type === 'folder',
            groupHeading: folderHeading,
        })}
    />
</div>

<style>
    .sync-grid {
        display: flex;
        flex: 1 1 auto;
        min-height: 0;
    }

    .name {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
    }

    .action-cell {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
    }

    .category {
        display: inline-flex;
        color: var(--color-text-tertiary);
    }

    .glyph {
        min-width: 2ch;
        font-weight: 600;
        text-align: center;
    }

    /* TC's colours, always beside the arrow's shape and the row's words. */
    .to-right {
        color: var(--color-allow);
    }

    .to-left {
        color: var(--color-accent-text);
    }

    .undecided {
        color: var(--color-warning-text);
    }

    .changed,
    .pending {
        color: var(--color-text-secondary);
    }

    .warning {
        display: inline-flex;
        color: var(--color-warning-text);
    }

    .rung {
        padding: 0 var(--spacing-xxs);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .mark {
        color: var(--color-accent-text);
    }

    .folder-heading {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        color: var(--color-text-secondary);
    }
</style>
