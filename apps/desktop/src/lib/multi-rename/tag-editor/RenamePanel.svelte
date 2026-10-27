<script lang="ts">
    /**
     * The Rename tab's body when the tab strip is on: the sheet's own column, one level down, as the strip's
     * tabpanel. With the tab strip off the dialog renders the body bare, so this never mounts.
     */
    import type { Snippet } from 'svelte'

    interface Props {
        children: Snippet
    }

    const { children }: Props = $props()
</script>

<div class="rename-panel" id="sheet-panel-rename" role="tabpanel" aria-labelledby="sheet-tab-rename">
    {@render children()}
</div>

<style>
    .rename-panel {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        flex: 1;
        min-height: 0;
    }
</style>
