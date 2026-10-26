<script lang="ts">
    // The AI suggestions dialog: what Ask Cmdr proposed, laid out so the user can decide.
    //
    // The layout is the argument. Each group shows the AI's reason under a label that says
    // whose words they are, next to facts Cmdr holds by itself, so a hallucinated claim is
    // visible AS a claim rather than reading like a finding. Reversibility and a folder that
    // doesn't exist yet are DISCLOSED here and never block: once the user approves, it is
    // exactly as if they started the action.
    //
    // Every group's file list shows right away, each over its own WINDOW the store fetches,
    // because a group of 60,000 ops is legitimate and neither a list nor the wire may grow
    // with it (`SuggestedOpFiles.svelte`).
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatDateTime } from '#lib/settings/reactive-settings.svelte.js'
    import { closeSuggestedOps, suggestedOpsState } from './suggested-ops-trigger.svelte'
    import AgentReason from './AgentReason.svelte'
    import SuggestedOpsGroup from './SuggestedOpsGroup.svelte'
</script>

<ModalDialog
    titleId="suggested-ops-title"
    dialogId="suggested-ops"
    role="dialog"
    onclose={closeSuggestedOps}
    containerStyle="width: 860px; max-width: calc(100vw - 2 * var(--spacing-xl))"
    resizable
>
    {#snippet title()}
        <span>{tString('suggestedOps.title')}</span>
    {/snippet}

    <p class="lede">{tString('suggestedOps.description')}</p>

    {#if suggestedOpsState.decisionNotice}
        <!-- Above the list rather than inside a group: after the re-read the group may be gone
             (its approval did land), and the notice still has to be read. -->
        <p class="decision-notice" role="alert">{tString(suggestedOpsState.decisionNotice)}</p>
    {/if}

    <!-- A re-read keeps the groups on screen: only the FIRST read has nothing to show yet.
         Blanking the list on every refresh would throw away where each list was scrolled. -->
    {#if suggestedOpsState.loading && suggestedOpsState.sweeps.length === 0}
        <p class="notice" role="status">{tString('suggestedOps.loadingFiles')}</p>
    {:else if suggestedOpsState.loadError && suggestedOpsState.sweeps.length === 0}
        <p class="notice notice-problem" role="alert">{tString('suggestedOps.loadFailed')}</p>
    {:else if suggestedOpsState.sweeps.length === 0}
        <p class="notice" role="status">{tString('suggestedOps.empty')}</p>
    {:else}
        {#each suggestedOpsState.sweeps as sweep (sweep.sweepId)}
            <section class="sweep">
                <header class="sweep-head">
                    <p class="sweep-meta">
                        {tString('suggestedOps.sweepMeta', { date: formatDateTime(sweep.createdAt) })}
                    </p>
                    {#if sweep.rationale}
                        <AgentReason text={sweep.rationale} />
                    {/if}
                </header>

                {#each sweep.groups as group (group.groupId)}
                    <SuggestedOpsGroup {group} />
                {/each}
            </section>
        {/each}
    {/if}
</ModalDialog>

<style>
    /* Every paragraph here sets its own margin: the global one is a prose rhythm. */
    .lede {
        margin: 0 0 var(--spacing-lg);
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
    }

    .notice {
        margin: var(--spacing-lg) 0;
        text-align: center;
        color: var(--color-text-secondary);
    }

    /* A load that didn't come back is the one notice the user has to act on, so it
       carries the error color instead of reading like the empty and loading states. */
    .notice-problem {
        color: var(--color-error-text);
    }

    /* The one notice above the list, so it reads as a banner over what follows, in the tint
       a group's own change notice uses for the same urgency. */
    .decision-notice {
        margin: 0 0 var(--spacing-lg);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        background: var(--color-error-bg);
        color: var(--color-error-text);
        font-size: var(--font-size-sm);
    }

    .sweep + .sweep {
        margin-top: var(--spacing-xl);
    }

    .sweep-head {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        padding-bottom: var(--spacing-lg);
    }

    .sweep-meta {
        margin: 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        font-weight: 500;
    }
</style>
