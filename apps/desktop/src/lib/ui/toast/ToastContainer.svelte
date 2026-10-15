<!--
    The toast stack, plus the two live regions it speaks through.

    ❗ The regions are always mounted, empty until a toast arrives (a region inserted with its
    first message announces unreliably in WebKit), and they're siblings: a polite one for
    status toasts and an assertive one for warnings and failures. The toasts themselves are
    no live regions, so nothing nests an `alert` inside a polite region and nothing speaks
    twice. `ui/DETAILS.md` § Toast system.
-->
<script lang="ts">
    import { onMount } from 'svelte'
    import { getToasts, dismissToast } from './toast-store.svelte'
    import ToastItem from './ToastItem.svelte'
    import type { ToastSpeech } from './toast-speech'
    import LiveAnnouncer from '../LiveAnnouncer.svelte'
    import { handleToastStackFocusOut, handleToastStackKeydown, registerToastStack } from './toast-focus'

    const toasts = getToasts()

    type Announcer = { announce: (message: string) => void }
    let politeRegion: Announcer | undefined = $state()
    let assertiveRegion: Announcer | undefined = $state()

    // Toasts raised in the same turn speak as one message per region: the announcer keeps
    // only the newest of two messages asked within a frame, which would drop the first.
    const pending = { polite: [] as string[], assertive: [] as string[] }
    let flushQueued = false

    function speak({ text, urgent }: ToastSpeech): void {
        if (!text) return
        pending[urgent ? 'assertive' : 'polite'].push(text)
        if (flushQueued) return
        flushQueued = true
        queueMicrotask(() => {
            flushQueued = false
            if (pending.polite.length > 0) politeRegion?.announce(pending.polite.splice(0).join(' '))
            if (pending.assertive.length > 0) assertiveRegion?.announce(pending.assertive.splice(0).join(' '))
        })
    }

    let stackElement: HTMLDivElement | undefined = $state()

    // The keyboard walk (`toast-focus.ts`). Listeners rather than `onkeydown` attributes: the
    // stack is no widget of its own, only the place a walk across its toasts happens.
    onMount(() => {
        const unregister = registerToastStack({
            element: () => stackElement,
            announce: (message) => politeRegion?.announce(message),
        })
        const element = stackElement
        element?.addEventListener('keydown', handleToastStackKeydown)
        element?.addEventListener('focusout', handleToastStackFocusOut)
        return () => {
            unregister()
            element?.removeEventListener('keydown', handleToastStackKeydown)
            element?.removeEventListener('focusout', handleToastStackFocusOut)
        }
    })

    function handleUserDismiss(id: string): void {
        const toast = toasts.find((t) => t.id === id)
        toast?.onDismiss?.()
        dismissToast(id)
    }
</script>

<div class="toast-container" data-toast-stack bind:this={stackElement}>
    {#each toasts as toast (toast.id)}
            <ToastItem
                id={toast.id}
                content={toast.content}
                level={toast.level}
                dismissal={toast.dismissal}
                timeoutMs={toast.timeoutMs}
                postedAt={toast.postedAt}
                closeTooltip={toast.closeTooltip}
                contentProps={toast.props}
                widthPx={toast.widthPx}
                report={toast.report}
                onTimeout={dismissToast}
                onUserDismiss={handleUserDismiss}
                onSpeak={speak}
            />
    {/each}
</div>
<LiveAnnouncer bind:this={politeRegion} data-toast-announcer="polite" />
<LiveAnnouncer bind:this={assertiveRegion} politeness="assertive" data-toast-announcer="assertive" />

<style>
    .toast-container {
        position: fixed;
        top: var(--spacing-lg);
        right: var(--spacing-lg);
        z-index: var(--z-notification);
        display: flex;
        flex-direction: column-reverse;
        /* Right-align so a per-toast `widthPx` (wider than the default 360) extends
           leftward while narrower toasts keep hugging the screen edge. The max-width
           gives the widest opt-in toast room; each ToastItem still caps itself at 360
           by default. */
        align-items: flex-end;
        /* Room for each toast's shadow to fall before the next one starts. */
        gap: var(--spacing-md);
        max-width: 440px;
        pointer-events: none;
    }

    .toast-container > :global(*) {
        pointer-events: auto;
    }
</style>
