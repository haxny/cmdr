<!--
    A pane's one polite live region: what VoiceOver should hear about the pane that no focused
    element says ("Left pane, Documents"). `FilePane` owns it and exposes `announce` on its API,
    so later messages (an empty folder, a selection count) reuse it instead of a second region.

    ❗ Mounted with the pane and empty: a region inserted together with its first message
    announces nothing. Each message clears the region and lands a frame later, so the same
    text twice in a row is a change both times.
-->
<script lang="ts">
    import { afterNextFrame } from '#lib/ui/deferred-active-descendant.svelte.js'

    let message = $state('')
    let cancel: (() => void) | null = null

    /** Speaks `next` politely (after whatever VoiceOver is saying). A newer message replaces a pending one. */
    export function announce(next: string): void {
        cancel?.()
        message = ''
        cancel = afterNextFrame(() => {
            cancel = null
            message = next
        })
    }

    $effect(() => () => cancel?.())
</script>

<div class="sr-only" aria-live="polite" aria-atomic="true" data-pane-announcer>{message}</div>
