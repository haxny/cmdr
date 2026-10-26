<script lang="ts">
    // One suggested group, top to bottom in the order a decision is made: what it is (title,
    // count, and the pills that change the answer), the facts Cmdr holds by itself (where the
    // files are and where they'd go), the AI's reason (labelled as its words), the files, and
    // the button that names the action.
    import Button from '#lib/ui/Button.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import type { SuggestedGroupView } from '#lib/tauri-commands/index.js'
    import {
        approvableCount,
        approveGroup,
        rejectGroup,
        reloadGroup,
        reviewOf,
        suggestedOpsState,
    } from './suggested-ops-trigger.svelte'
    import { refusalReason } from './suggested-ops-refusal'
    import { approveKey, reversibilityKey } from './suggested-ops-copy'
    import AgentReason from './AgentReason.svelte'
    import SuggestedOpFiles from './SuggestedOpFiles.svelte'

    interface Props {
        group: SuggestedGroupView
    }

    const { group }: Props = $props()

    const review = $derived(reviewOf(group.groupId))
    const count = $derived(approvableCount(group.groupId))
    const busy = $derived(suggestedOpsState.busyGroupId !== null)
    /** The reason under a group whose last approval didn't start, or `null`. */
    const refusal = $derived.by(() => {
        const answer = suggestedOpsState.refusals.get(group.groupId)
        return answer ? refusalReason(answer, group.verb) : null
    })
    const irreversible = $derived(group.reversible === 'irreversible')
    const titleId = $derived(`suggested-group-${String(group.groupId)}-title`)
</script>

<article class="group" aria-labelledby={titleId}>
    <header class="group-head">
        <h3 class="group-title" id={titleId}>{group.displayName}</h3>
        <span class="count">
            {tString('suggestedOps.fileCount', { countText: formatInteger(group.liveOpCount), count: group.liveOpCount })}
        </span>
        <ul class="pills">
            <li class="pill" class:danger={irreversible}>
                {tString(reversibilityKey(group.verb, group.reversible), { count: group.liveOpCount })}
            </li>
            {#if group.destinationState === 'willBeCreated'}
                <li class="pill" use:tooltip={tString('suggestedOps.folderWillBeCreatedTooltip')}>
                    {tString('suggestedOps.folderWillBeCreated')}
                </li>
            {:else if group.destinationState === 'notConnected'}
                <li class="pill" use:tooltip={tString('suggestedOps.destinationNotConnectedTooltip')}>
                    {tString('suggestedOps.destinationNotConnected')}
                </li>
            {:else if group.destinationState === 'unknown'}
                <li class="pill">{tString('suggestedOps.destinationUnknown')}</li>
            {/if}
            {#if group.fromSelector}
                <li class="pill">{tString('suggestedOps.fromPattern')}</li>
            {/if}
        </ul>
    </header>

    <!-- Facts Cmdr holds by itself, unlabelled as anyone's words: they're the check the AI's
         reason below can be read against. -->
    <dl class="facts selectable">
        <dt>{tString(group.verb === 'rename' ? 'suggestedOps.facts.folder' : 'suggestedOps.facts.from')}</dt>
        <dd class="path">{group.sourceFolder ?? tString('suggestedOps.facts.severalFolders')}</dd>
        {#if group.verb === 'trash'}
            <dt>{tString('suggestedOps.facts.to')}</dt>
            <dd>{tString('suggestedOps.facts.trash')}</dd>
        {:else if group.destination !== null && group.verb !== 'rename' && group.verb !== 'delete'}
            <dt>{tString(group.verb === 'compress' ? 'suggestedOps.facts.archive' : 'suggestedOps.facts.to')}</dt>
            <dd>
                {#if group.destinationVolumeName}<span class="volume">{group.destinationVolumeName}</span>{/if}
                <span class="path">{group.destination}</span>
            </dd>
        {/if}
    </dl>

    {#if group.rationale}
        <AgentReason text={group.rationale} />
    {/if}

    {#if review?.changedUnderReview}
        <!-- The rows stay exactly where they are. Re-ordering a list somebody is halfway
             through deciding on is how a wrong row gets approved. -->
        <div class="changed" role="status">
            <span>{tString('suggestedOps.changedUnderReview')}</span>
            <Button size="mini" onclick={() => void reloadGroup(group.groupId)}>
                {tString('suggestedOps.showNewVersion')}
            </Button>
        </div>
    {/if}

    <SuggestedOpFiles {group} />

    <!-- eslint-disable-next-line cmdr/dialog-buttons-in-footer -- each suggestion's own Reject / approve pair, one per group in the list; the dialog has no footer actions of its own -->
    <div class="actions">
        <Button disabled={busy} onclick={() => void rejectGroup(group.groupId)}>
            {tString('suggestedOps.reject')}
        </Button>
        <Button
            variant={irreversible ? 'danger' : 'primary'}
            disabled={busy || count === 0}
            onclick={() => void approveGroup(group.groupId)}
        >
            {tString(approveKey(group.verb), { countText: formatInteger(count), count })}
        </Button>
    </div>

    {#if refusal}
        <!-- An approval that didn't start gave the group back: it's pending again and nothing
             ran, so the reason sits right under the button the user just pressed. -->
        <div class="refusal" role="alert">
            <span class="refusal-label">{tString('suggestedOps.refusal.label')}</span>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- markup from the typed error via `refusalReason`: the write-error pipeline escapes every name and path. Same boundary as the queue's failed row. -->
            <p class="refusal-message selectable">{@html refusal.message}</p>
            <p class="refusal-suggestion selectable">{refusal.suggestion}</p>
        </div>
    {/if}
</article>

<style>
    .group {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        padding-block: var(--spacing-lg);
        border-top: 1px solid var(--color-border-subtle);
    }

    .group-head {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        column-gap: var(--spacing-sm);
        row-gap: var(--spacing-xs);
    }

    .group-title {
        font-size: var(--font-size-md);
        font-weight: 600;
    }

    .count {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .pills {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
        list-style: none;
    }

    .pill {
        padding: var(--spacing-xxs) var(--spacing-sm);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-full);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-tight);
        white-space: nowrap;
    }

    /* "Can't be undone" has to look different from the neutral pills beside it: it's the one
       that decides whether the user approves the group. */
    .pill.danger {
        border-color: var(--color-error-border);
        background: var(--color-error-bg);
        color: var(--color-error-text);
    }

    .facts {
        display: grid;
        grid-template-columns: max-content minmax(0, 1fr);
        column-gap: var(--spacing-sm);
        row-gap: var(--spacing-xxs);
        font-size: var(--font-size-sm);
    }

    .facts dt {
        color: var(--color-text-tertiary);
    }

    .facts dd {
        color: var(--color-text-primary);
    }

    .volume {
        margin-right: var(--spacing-xs);
        font-weight: 600;
    }

    .path {
        overflow-wrap: anywhere;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
    }

    .changed {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        background: var(--color-warning-bg);
        color: var(--color-warning-text);
        font-size: var(--font-size-sm);
    }

    .refusal {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        font-size: var(--font-size-sm);
    }

    .refusal-label {
        color: var(--color-text-secondary);
        font-weight: 600;
    }

    .refusal-message {
        color: var(--color-error-text);
    }

    .refusal-suggestion {
        color: var(--color-text-secondary);
    }
</style>
