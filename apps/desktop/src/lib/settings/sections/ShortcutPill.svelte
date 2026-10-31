<script lang="ts">
    /**
     * A shortcut pill in the Keyboard shortcuts section: the chip of a
     * `KeyboardShortcutsSection` row and of `#lib/downloads/GlobalShortcutRow.svelte`, which
     * sits in the same list and has to look the same. Editable by default (a button that
     * starts recording); `readOnly` renders the same shape as a plain span for rows Cmdr
     * can't rebind. The prose-side sibling is `#lib/ui/ShortcutChip`, which only displays.
     */
    import type { Snippet } from 'svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'

    interface Props {
        /** Shown, not editable: a plain span, no hover, no click (macOS-native and fixed-key rows). */
        readOnly?: boolean
        /** Recording a combo: accent fill. */
        editing?: boolean
        /**
         * Recording, and the captured combo conflicts: warning tint (also on hover) while the
         * banner awaits a decision, so it reads as "in question", not as a saved binding.
         */
        pendingConflict?: boolean
        /** An unbound slot: dashed border, muted text. */
        empty?: boolean
        onclick?: () => void
        /**
         * Adds the × that removes this binding: a sibling button of the pill, shown on hover and
         * while the pill or the × has focus. Omit for slots that can't be removed.
         */
        remove?: { tooltip: string; onRemove: () => void }
        /** Test hook, rendered as `data-test`. */
        dataTest?: string
        children: Snippet
    }

    const { readOnly = false, editing = false, pendingConflict = false, empty = false, onclick, remove, dataTest, children }: Props =
        $props()
</script>

{#if readOnly}
    <span class="shortcut-pill static">{@render children()}</span>
{:else}
    <span class="shortcut-pill-group" class:removable={remove !== undefined}>
        <button class="shortcut-pill" class:editing class:pending-conflict={pendingConflict} class:empty data-test={dataTest} {onclick}>
            {@render children()}
        </button>
        {#if remove}
            <button
                type="button"
                class="remove-shortcut"
                aria-label={remove.tooltip}
                use:tooltip={remove.tooltip}
                onclick={remove.onRemove}
            >
                <span aria-hidden="true">×</span>
            </button>
        {/if}
    </span>
{/if}

<style>
    /* The × is the pill's sibling (ARIA makes a button's children presentational), drawn over
       the pill's trailing edge so it still reads as inside it. Revealing it grows the pill by
       the room the × takes, as it did when the × was a flex child. */
    .shortcut-pill-group {
        position: relative;
        display: inline-flex;
    }

    .shortcut-pill-group.removable:hover .shortcut-pill,
    .shortcut-pill-group.removable:focus-within .shortcut-pill {
        padding-inline-end: calc(var(--spacing-sm) + var(--spacing-md) + var(--spacing-xs));
    }

    .shortcut-pill {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-family: var(--font-system) sans-serif;
        color: var(--color-text-primary);
        cursor: default;
        min-width: 40px;
        text-align: center;
    }

    .shortcut-pill.editing {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border-color: var(--color-accent);
    }

    .shortcut-pill.editing:hover {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }

    .shortcut-pill.editing.pending-conflict,
    .shortcut-pill.editing.pending-conflict:hover {
        background: var(--color-warning-bg);
        color: var(--color-text-primary);
        border-color: var(--color-warning);
    }

    .shortcut-pill.empty {
        color: var(--color-text-tertiary);
        border-style: dashed;
    }

    .shortcut-pill.static {
        color: var(--color-text-secondary);
    }

    /* Painted 12px. Hidden with `opacity`, ❌ never `display: none`: Tab has to reach it. */
    .remove-shortcut {
        position: absolute;
        top: 50%;
        right: calc(var(--spacing-sm) + 1px);
        transform: translateY(-50%);
        width: var(--spacing-md);
        height: var(--spacing-md);
        padding: 0;
        border: none;
        border-radius: var(--radius-full);
        background: var(--color-text-tertiary);
        color: var(--color-bg-primary);
        font-size: var(--font-size-xs);
        font-weight: 600;
        cursor: default;
        display: flex;
        align-items: center;
        justify-content: center;
        line-height: var(--font-line-height-flat);
        opacity: 0;
        pointer-events: none;
    }

    /* The unpainted hit box: 24 by 24 around the 12px circle (WCAG 2.5.8). */
    .remove-shortcut::before {
        content: '';
        position: absolute;
        inset: -6px;
    }

    .shortcut-pill-group:hover .remove-shortcut,
    .shortcut-pill-group:focus-within .remove-shortcut {
        opacity: 1;
        pointer-events: auto;
    }
</style>
