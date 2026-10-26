<script lang="ts">
    // The AI's words, visibly ITS words. The label isn't decoration: an unlabelled reason reads
    // as something Cmdr checked, and a hallucinated claim then arrives looking like a finding.
    // The label sits at the text's own size and weight above it, so it reads as a heading for
    // the quote rather than as a footnote to it.
    import { tString } from '#lib/intl/messages.svelte.js'

    interface Props {
        text: string
    }

    const { text }: Props = $props()
</script>

<figure class="agent-reason">
    <figcaption class="agent-label">{tString('suggestedOps.agentReason')}</figcaption>
    <blockquote class="agent-text selectable">{text}</blockquote>
</figure>

<style>
    .agent-reason {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        padding-left: var(--spacing-sm);
        border-left: 2px solid var(--color-border);
        font-size: var(--font-size-sm);
    }

    .agent-label {
        color: var(--color-text-secondary);
        font-weight: 600;
    }

    .agent-text {
        color: var(--color-text-secondary);
    }
</style>
