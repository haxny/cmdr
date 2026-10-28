<!--
  Test-only host for `DialogBoundary`, in the shape every route uses: the
  boundary wraps the `{#if}` that mounts the dialog, beside a marker that proves
  the rest of the window survived.

  `view` is a `$state` object the test owns, so it can open, close, and swap the
  dialog between flushes. `onClose` is the test's own, so it decides whether
  "closing" really closes (the re-arm cap test keeps it open on purpose).
  `withGaveUp` hands the boundary a `gaveUp` fallback and its toast text.
-->
<script lang="ts">
    import type { Component } from 'svelte'
    import DialogBoundary from '#lib/ui/boundary/DialogBoundary.svelte'

    interface Props {
        view: { open: boolean; dialog: Component }
        onClose: () => void
        withGaveUp?: boolean
    }

    const { view, onClose, withGaveUp = false }: Props = $props()
    const Dialog = $derived(view.dialog)
</script>

<p data-testid="window-alive">The window is still here</p>
{#if withGaveUp}
    <DialogBoundary name="fixture" {onClose} gaveUpMessage="It stopped reloading">
        {#if view.open}
            <Dialog />
        {/if}
        {#snippet gaveUp()}
            <p data-testid="gave-up">It broke for good</p>
        {/snippet}
    </DialogBoundary>
{:else}
    <DialogBoundary name="fixture" {onClose}>
        {#if view.open}
            <Dialog />
        {/if}
    </DialogBoundary>
{/if}
