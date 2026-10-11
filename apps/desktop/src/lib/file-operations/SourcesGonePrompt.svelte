<script lang="ts">
    /**
     * Mounts `SourcesGoneDialog` for the question `sources-gone.svelte.ts` has on
     * screen. Mounted unconditionally by the main page, so the page stays a
     * one-liner and the host owns visibility.
     */
    import SourcesGoneDialog from './SourcesGoneDialog.svelte'
    import {
        cancelSourcesGonePrompt,
        continueSourcesGonePrompt,
        getSourcesGonePrompt,
        isAnsweringSourcesGone,
    } from './sources-gone.svelte'

    const prompt = $derived(getSourcesGonePrompt())
</script>

{#if prompt}
    {#key prompt.operationId}
        <SourcesGoneDialog
            event={prompt.event}
            snapshot={prompt.snapshot}
            answering={isAnsweringSourcesGone()}
            onContinue={() => void continueSourcesGonePrompt()}
            onCancel={() => void cancelSourcesGonePrompt()}
        />
    {/key}
{/if}
