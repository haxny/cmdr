<!--
    A live region: what a screen reader should hear that no focused element says (a file
    pane's "Left pane, Documents", a search's "3 sections match"). Polite unless the owner asks
    for `politeness="assertive"` (the toast stack's warnings and failures). The owner calls
    `announce`; any extra attributes (`data-*` hooks) land on the region.

    ❗ Mounted with its owner and empty: a region inserted together with its first message
    announces nothing. Each message clears the region and lands a frame later, so the same
    text twice in a row is a change both times. Debouncing a burst is the caller's job: this
    only replaces a message still waiting for its frame.
-->
<script lang="ts">
    import type { HTMLAttributes } from 'svelte/elements'
    import { afterNextFrame } from './deferred-active-descendant.svelte'

    interface Props extends HTMLAttributes<HTMLDivElement> {
        /** `assertive` interrupts the reader; keep it for news that can't wait. */
        politeness?: 'polite' | 'assertive'
    }

    const { politeness = 'polite', ...rest }: Props = $props()

    let message = $state('')
    let cancel: (() => void) | null = null

    /** Speaks `next` at the region's politeness. A newer message replaces a pending one. */
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

<div {...rest} class="sr-only" aria-live={politeness} aria-atomic="true" data-live-announcer>{message}</div>
