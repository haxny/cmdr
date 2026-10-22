<script lang="ts">
    /**
     * A toast's action row: every button a toast offers sits in one of these, string toasts'
     * "Send error report…" and component bodies alike (`toast-body-layout.test.ts` holds bodies
     * to it). One row, so a long locale can't break a label in one toast and not another.
     *
     * The row asks for its natural one-line width, and `ToastItem` grows past its default width
     * to give it that (`ui/DETAILS.md` § Toast system). Only a row wider than the window allows
     * wraps, and then whole buttons move to the next line, never part of a label.
     */
    import type { Snippet } from 'svelte'

    interface Props {
        children: Snippet
    }

    const { children }: Props = $props()
</script>

<div class="toast-actions">
    {@render children()}
</div>

<style>
    /* `width: max-content` makes the row's one-line width its MINIMUM width too, which is what
       lets the toast's `min-width: min-content` grow to it. `--spacing-toast-actions-max` (`app.css`) caps that at what the window allows; past it, `flex-wrap` moves whole buttons
       down. `margin-left: auto` keeps the row right-aligned when the toast is wider anyway. */
    .toast-actions {
        display: flex;
        flex-wrap: wrap;
        justify-content: flex-end;
        align-items: center;
        gap: var(--spacing-sm);
        width: max-content;
        max-width: var(--spacing-toast-actions-max);
        margin-top: var(--spacing-md);
        margin-left: auto;
        /* The frame's `overflow-wrap: anywhere` (for long paths in the text) would otherwise
           let a label break mid-word; `Button` resets it too, this covers link-shaped actions. */
        overflow-wrap: normal;
        white-space: nowrap;
    }
</style>
