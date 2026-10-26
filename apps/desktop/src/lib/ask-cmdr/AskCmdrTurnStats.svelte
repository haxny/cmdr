<!--
  The quiet line under one assistant turn saying what it spent: tool calls, wall time, and
  tokens. Shown for any turn that ran a tool, answered, failed, or stopped alike: what the
  agent spent matters most when it didn't deliver. Named apart from `AskCmdrCostFooter`, which
  sums the whole thread; the tokens here are the same per-row figures, so the turns add up to it.
-->
<script lang="ts">
    import type { TurnStats } from '#lib/ipc/bindings.js'
    import { turnStatsText } from './ask-cmdr-labels'

    interface Props {
        stats: TurnStats | null
    }
    const { stats }: Props = $props()

    const text = $derived(turnStatsText(stats))
</script>

{#if text !== null}
    <div class="turn-stats">{text}</div>
{/if}

<style>
    .turn-stats {
        padding: 0 var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
