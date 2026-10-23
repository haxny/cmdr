<script lang="ts">
    /**
     * The chevron beside the Full date header: it opens the native Sort by popup (Date
     * modified, Date created, Date added) anchored under itself, the keyboard's and the
     * mouse's way to every date sort without leaving the header. `FieldHistoryChevron`'s
     * look, sized to the header row.
     *
     * A plain button, ❌ no `tabindex`: pane chrome's ←/→ walk finds it on its own
     * (`pane/pane-chrome.ts`), and an explicit one makes WebKit focus it on a mouse click.
     * The measurer mirrors its width (`HEADER_DATE_SORT_CHEVRON`); change them together.
     */
    import Icon from '#lib/ui/Icon.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import type { MenuAnchor } from '#lib/tauri-commands/index.js'

    interface Props {
        /** Opens the menu at `anchor` (viewport CSS px); resolves once it closes. */
        onOpen: (anchor: MenuAnchor) => Promise<void>
    }

    const { onOpen }: Props = $props()

    /** True while the popup is up: the show command blocks in Rust until it closes. */
    let expanded = $state(false)

    async function open(button: HTMLElement): Promise<void> {
        if (expanded) return
        const rect = button.getBoundingClientRect()
        expanded = true
        try {
            await onOpen({ x: rect.left, y: rect.bottom })
        } finally {
            expanded = false
        }
    }

    // Enter / Space open the menu here, with no synthesized click: a click would bubble to the
    // pane's `onclick` and pull focus back to the list, and the next press should find focus
    // still on the chevron (`SortableHeader` does the same).
    function handleKeyDown(e: KeyboardEvent): void {
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            void open(e.currentTarget as HTMLElement)
        }
    }
</script>

<button
    type="button"
    class="date-sort-chevron"
    aria-label={tString('fileExplorer.columns.dateSortMenu')}
    aria-haspopup="menu"
    aria-expanded={expanded}
    onclick={(e: MouseEvent) => void open(e.currentTarget as HTMLElement)}
    onkeydown={handleKeyDown}
    use:tooltip={tString('fileExplorer.columns.dateSortMenu')}
>
    <Icon name="chevron-down" size={12} aria-hidden="true" />
</button>

<style>
    .date-sort-chevron {
        display: inline-flex;
        flex-shrink: 0;
        align-items: center;
        justify-content: center;
        height: 100%;
        /* The padding gives the hover background room; the equal negative margin keeps the
           layout width at the glyph's, which is what the measurer reserves. */
        padding: 0 var(--spacing-xs);
        margin: 0 calc(-1 * var(--spacing-xs));
        border: none;
        border-radius: var(--radius-sm);
        background: transparent;
        /* The header labels' color, so the glyph keeps their contrast. */
        color: var(--color-text-secondary);
        transition:
            background-color var(--transition-fast),
            color var(--transition-fast);
    }

    .date-sort-chevron:hover,
    .date-sort-chevron[aria-expanded='true'] {
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
    }
</style>
