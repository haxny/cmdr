<script lang="ts">
    /**
     * TC's show-filter buttons above the grid (→ = ≠ ← duplicates singles, plus ? and warnings), each a toggle chip
     * with its ⌘⇧ key. A button that hides rows shows "!", as TC's do, so hidden rows are never a surprise. Hiding a
     * row never changes its action (the summary counts hidden actions).
     */
    import Chip from '#lib/ui/Chip.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { ShowFilters } from '#lib/tauri-commands/index.js'
    import { filterHides } from './sync-options'

    interface Props {
        filters: ShowFilters
        onToggle: (key: keyof ShowFilters) => void
    }

    const { filters, onToggle }: Props = $props()

    interface FilterButton {
        key: keyof ShowFilters
        glyph: string
        label: MessageKey
        shortcut: string
    }

    const BUTTONS: FilterButton[] = [
        { key: 'toRight', glyph: '→', label: 'sync.filters.toRight', shortcut: '⌘⇧L' },
        { key: 'equal', glyph: '=', label: 'sync.filters.equal', shortcut: '⌘⇧E' },
        { key: 'different', glyph: '≠', label: 'sync.filters.different', shortcut: '⌘⇧N' },
        { key: 'toLeft', glyph: '←', label: 'sync.filters.toLeft', shortcut: '⌘⇧R' },
        { key: 'duplicates', glyph: '⇄', label: 'sync.filters.duplicates', shortcut: '⌘⇧D' },
        { key: 'singles', glyph: '◧', label: 'sync.filters.singles', shortcut: '⌘⇧S' },
        { key: 'unknown', glyph: '?', label: 'sync.filters.unknown', shortcut: '⌘⇧U' },
        { key: 'warningsOnly', glyph: '⚠', label: 'sync.filters.warningsOnly', shortcut: '⌘⇧W' },
    ]
</script>

<div class="filter-bar" role="group" aria-label={tString('sync.filters.label')}>
    {#each BUTTONS as button (button.key)}
        {@const hides = filterHides(filters, button.key)}
        <Chip
            variant="toggle"
            pressed={filters[button.key]}
            label={tString(button.label)}
            tooltipContent={{ text: hides ? tString('sync.filters.hiddenMark') : tString(button.label), shortcut: button.shortcut }}
            onActivate={() => {
                onToggle(button.key)
            }}
        >
            {#snippet leading()}
                <span class="glyph" aria-hidden="true">{button.glyph}</span>{#if hides}<span
                        class="hidden-mark"
                        aria-hidden="true">!</span
                    >{/if}
            {/snippet}
        </Chip>
    {/each}
</div>

<style>
    .filter-bar {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
    }

    .glyph {
        font-weight: 600;
    }

    .hidden-mark {
        color: var(--color-warning-text);
        font-weight: 600;
    }
</style>
