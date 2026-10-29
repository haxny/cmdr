<!--
  The card a running answer shows when its estimated spend passed the user's threshold (Settings
  › AI › Ask Cmdr): what it has used so far, and Keep going or Stop. The turn waits between rounds
  and spends nothing until the user answers. The figure is the same list-price estimate the cost
  footer shows, which ignores prompt caching, so the note under it says the real bill is usually
  lower.
-->
<script lang="ts">
    import Icon from '#lib/ui/Icon.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatUsdMicros } from './ask-cmdr-cost'

    interface Props {
        costMicros: number
        onKeepGoing: () => void
        onStop: () => void
    }
    const { costMicros, onKeepGoing, onStop }: Props = $props()

    const question = $derived(tString('askCmdr.spendPause.question', { cost: formatUsdMicros(costMicros) }))
</script>

<div class="spend-pause" role="group" aria-label={question}>
    <Icon name="circle-pause" size={14} aria-hidden="true" />
    <div class="stack">
        <span role="status">{question}</span>
        <span class="note">{tString('askCmdr.spendPause.estimateNote')}</span>
        <div class="actions">
            <Button size="mini" variant="primary" onclick={onKeepGoing}>{tString('askCmdr.spendPause.keepGoing')}</Button>
            <Button size="mini" variant="secondary" onclick={onStop}>{tString('askCmdr.spendPause.stop')}</Button>
        </div>
    </div>
</div>

<style>
    .spend-pause {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-md);
    }

    .spend-pause > :global(svg) {
        flex-shrink: 0;
        margin-top: var(--spacing-xxs);
    }

    .stack {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .note {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .actions {
        display: flex;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-xxs);
    }
</style>
