<script lang="ts">
    /**
     * Mounts `LockedSourcesDialog` for the question `locked-sources.svelte.ts` has
     * on screen. Mounted unconditionally by the main page, so the page stays a
     * one-liner and the host owns visibility.
     */
    import LockedSourcesDialog from './LockedSourcesDialog.svelte'
    import {
        cancelLockedSourcesPrompt,
        getLockedSourcesPrompt,
        isAnsweringLockedSources,
        moveLockedSourcesAnyway,
        skipLockedSources,
    } from './locked-sources.svelte'

    const prompt = $derived(getLockedSourcesPrompt())
</script>

{#if prompt}
    {#key prompt.operationId}
        <LockedSourcesDialog
            event={prompt.event}
            snapshot={prompt.snapshot}
            answering={isAnsweringLockedSources()}
            onMoveAnyway={() => void moveLockedSourcesAnyway()}
            onSkip={() => void skipLockedSources()}
            onCancel={() => void cancelLockedSourcesPrompt()}
        />
    {/key}
{/if}
