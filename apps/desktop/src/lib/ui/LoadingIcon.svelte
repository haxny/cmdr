<script lang="ts">
    import ShortcutChip from '#lib/ui/ShortcutChip.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import ProgressBar from '#lib/ui/ProgressBar.svelte'
    import { sizeDisplayText } from '#lib/ui/size-display.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import type { ListingPhase } from '#lib/ipc/bindings.js'
    import type { Snippet } from 'svelte'

    interface Props {
        openingFolder?: boolean
        loadedCount?: number
        /** Work the listing does before its first entry, with its own measure (a backup's index load). */
        phase?: ListingPhase | null
        finalizingCount?: number
        showCancelHint?: boolean
    }

    const {
        openingFolder = false,
        loadedCount,
        phase = null,
        finalizingCount,
        showCancelHint = false,
    }: Props = $props()

    const formatNumber = formatInteger

    /** "48 MB of 312 MB", in the live rounded form a readout that moves several times a second uses. */
    const phaseAmountText = $derived(
        phase
            ? tString('ui.loadingIcon.backupIndexAmount', {
                  doneText: sizeDisplayText(phase.doneBytes, true),
                  totalText: sizeDisplayText(phase.totalBytes, true),
              })
            : '',
    )
    // An index with nothing to read has no fraction; an empty bar beats NaN.
    const phaseFraction = $derived(phase && phase.totalBytes > 0 ? phase.doneBytes / phase.totalBytes : 0)
</script>

{#snippet escKeyChip(children: Snippet)}<ShortcutChip key="Esc" />{@render children()}{/snippet}

<div class="loading-container">
    <Spinner size="lg" />
    {#if finalizingCount !== undefined}
        <div class="loading-text">
            {tString('ui.loadingIcon.finalizing', { countText: formatNumber(finalizingCount), count: finalizingCount })}
        </div>
    {:else if phase}
        <div class="loading-text">{tString('ui.loadingIcon.loadingBackupIndex', { amountText: phaseAmountText })}</div>
        <div class="phase-bar">
            <ProgressBar
                value={phaseFraction}
                ariaLabel={tString('ui.loadingIcon.backupIndexAria')}
                ariaValuetext={phaseAmountText}
            />
        </div>
    {:else if loadedCount !== undefined}
        <div class="loading-text">
            {tString('ui.loadingIcon.loaded', { countText: formatNumber(loadedCount), count: loadedCount })}
        </div>
    {:else if openingFolder}
        <div class="loading-text">{tString('ui.loadingIcon.openingFolder')}</div>
    {:else}
        <div class="loading-text">{tString('ui.loadingIcon.loading')}</div>
    {/if}
    {#if showCancelHint}
        <div class="cancel-hint">
            <Trans key="ui.loadingIcon.cancelHint" snippets={{ key: escKeyChip }} />
        </div>
    {/if}
</div>

<style>
    .loading-container {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-lg);
        width: 100%;
        height: 100%;
        animation: fadeIn 400ms ease-in;
    }

    @keyframes fadeIn {
        0%,
        50% {
            opacity: 0;
        }
        100% {
            opacity: 1;
        }
    }

    .loading-text {
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
    }

    /* The bar is `flex: 1` inside a row; this row gives it a readable width under the text. */
    .phase-bar {
        display: flex;
        width: min(320px, 80%);
    }

    .cancel-hint {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
