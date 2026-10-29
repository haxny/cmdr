<script lang="ts">
    /**
     * The Tracks pane's filter field (⌘F, `tags.filter`), above its column header: typing narrows the list
     * (`track-filter.ts`), and its count says how many tracks show and how many selected ones it hides (seen here; the
     * pane's always-present live region announces it). Esc clears and closes it; Enter or ↓ goes back to the list, the
     * filter staying on.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import TextInput from '#lib/ui/TextInput.svelte'
    import { filterCountText } from './track-filter-text'

    interface Props {
        text: string
        /** How many tracks the filter shows, of `total`. */
        shownCount: number
        total: number
        /** Selected tracks the filter hides. */
        hiddenCount: number
        onChange: (text: string) => void
        /** Esc: clears and closes the filter. */
        onClose: () => void
        /** Enter or ↓: back to the list. */
        onLeave: () => void
    }

    const { text, shownCount, total, hiddenCount, onChange, onClose, onLeave }: Props = $props()

    let input = $state<HTMLInputElement>()

    /** Focuses the field with its text selected (⌘F from the list), so typing starts a new filter. */
    export function focus(): void {
        input?.focus()
        input?.select()
    }

    const countText = $derived(filterCountText(shownCount, total, hiddenCount))

    function onkeydown(e: KeyboardEvent): void {
        if (e.isComposing || e.shiftKey || e.altKey || e.metaKey || e.ctrlKey) return
        if (e.key === 'Escape') {
            // Claimed, or the window's Escape (`tags.close`) would close the window.
            claimKey(e)
            onClose()
        } else if (e.key === 'Enter' || e.key === 'ArrowDown') {
            claimKey(e)
            onLeave()
        }
    }
</script>

<div class="filter-bar">
    <TextInput
        bind:inputElement={input}
        value={text}
        radius="md"
        leadingIcon="search"
        ariaLabel={tString('multiRename.tags.filter.label')}
        placeholder={tString('multiRename.tags.filter.placeholder')}
        autocomplete="off"
        spellcheck={false}
        containerStyle="flex: 1; min-width: 0"
        oninput={(e: Event & { currentTarget: HTMLInputElement }) => { onChange(e.currentTarget.value) }}
        {onkeydown}
    />
    <span class="filter-count" aria-hidden="true">{countText}</span>
</div>

<style>
    .filter-bar {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .filter-count {
        flex-shrink: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        font-variant-numeric: tabular-nums;
    }
</style>
