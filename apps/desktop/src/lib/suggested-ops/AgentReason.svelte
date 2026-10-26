<script lang="ts">
    // The AI's words, visibly ITS words. The bold "Why:" leads the same line and its tooltip says
    // whose words follow, so the reason never reads as something Cmdr checked: an unlabelled
    // reason would let a hallucinated claim arrive looking like a finding.
    import { tString } from '#lib/intl/messages.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'

    interface Props {
        text: string
    }

    const { text }: Props = $props()
</script>

<p class="agent-reason selectable">
    <!-- Screen readers get the label that names the AI; the eye gets the short "Why:" and its tooltip. -->
    <strong class="agent-label" aria-hidden="true" use:tooltip={tString('suggestedOps.agentReasonTooltip')}
        >{tString('suggestedOps.agentReason')}</strong
    >
    <span class="sr-only">{tString('suggestedOps.agentReasonSpoken')}</span>
    <span>{text}</span>
</p>

<style>
    .agent-reason {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .agent-label {
        color: var(--color-text-primary);
        font-weight: 600;
    }
</style>
