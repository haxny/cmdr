<script lang="ts">
    /**
     * The Tags tab of the Multi-Rename sheet. For now a placeholder: it opens the tag session the first time it
     * shows, then says how many tracks it found. The tag table replaces it.
     */
    import { onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { TagEditorState } from './tag-editor-state.svelte'

    interface Props {
        state: TagEditorState
    }

    const { state: tags }: Props = $props()

    onMount(() => {
        void tags.open()
    })
</script>

<div class="tags-tab" id="sheet-panel-tags" role="tabpanel" aria-labelledby="sheet-tab-tags">
    {#if tags.error !== null && tags.view === null}
        <p role="alert">{tString('multiRename.tags.error')}</p>
    {:else if tags.view === null}
        <p role="status">{tString('multiRename.tags.reading')}</p>
    {:else}
        <p>
            {tString('multiRename.tags.trackCount', {
                count: tags.view.tracks.length,
                countText: formatInteger(tags.view.tracks.length),
            })}
        </p>
    {/if}
</div>

<style>
    .tags-tab {
        flex: 1;
        min-height: 0;
        padding: var(--spacing-md);
        color: var(--color-text-secondary);
    }
</style>
