<!--
    The stalled-transfer notice at the foot of the progress dialog: why the
    transfer stopped moving (the BACKEND's verdict), how many files sit in
    flight, and an offer to report it. Full content width, warning-toned, and
    built from the house card so it can't drift from the conflict block in
    `TransferDialog`. `DETAILS.md` § "The stalled-transfer notice".
-->
<script lang="ts">
    import type { TransferOperationType } from '#lib/file-explorer/types.js'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import ReportOffer from '#lib/error-reporter/ReportOffer.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { stallIncidentFor, type StallNotice } from './transfer-stall'

    interface Props {
        stall: StallNotice
        operationType: TransferOperationType
    }

    const { stall, operationType }: Props = $props()
</script>

<div class="stall-notice">
    <SectionCard tone="warning">
        <div class="stall-body" role="status">
            <span class="stall-icon" aria-hidden="true"><Icon name="hourglass" size={16} /></span>
            <div class="stall-text">
                <p class="stall-reason">
                    {#if stall.reason === 'destination'}
                        {tString('fileOperations.transferProgress.stallWaitingDestination')}
                    {:else if stall.reason === 'source'}
                        {tString('fileOperations.transferProgress.stallWaitingSource')}
                    {:else}
                        {tString('fileOperations.transferProgress.stallUnknown')}
                    {/if}
                </p>
                {#if stall.inFlight > 0}
                    <!-- Why the finished count can read lower than what
                         the user can see at the destination. -->
                    <p class="stall-detail">
                        {tString('fileOperations.transferProgress.stallInFlight', {
                            count: stall.inFlight,
                        })}
                    </p>
                {/if}
                <p class="stall-detail">
                    {tString('fileOperations.transferProgress.stallLogHint')}
                    <ReportOffer incident={stallIncidentFor(stall, operationType)} />
                </p>
            </div>
        </div>
    </SectionCard>
</div>

<style>
    /* Full content width: no side inset of its own, so the card lines up with
       the readout above it instead of sitting in from both edges. The card
       carries the tint, the border, and its own generous padding; all this
       wrapper owns is the gap below, which the button row's top padding
       already supplies. */
    .stall-notice :global(.section-card-wrap) {
        margin-bottom: 0;
    }

    .stall-body {
        display: flex;
        gap: var(--spacing-md);
        align-items: flex-start;
    }

    /* The one colored mark on the card. A tone colors the SURFACE and text keeps
       its own color (`#lib/ui/DETAILS.md` § SectionCard), so the icon is where
       the warning reads as a warning. `--color-warning-text`, not
       `--color-warning`: the brand orange clocks ~3.3:1 on this tint. */
    .stall-icon {
        color: var(--color-warning-text);
        flex-shrink: 0;
        display: flex;
        align-items: center;
        /* Optically centered on the first line of the reason, whose line box is
           taller than the 16px glyph. */
        padding-top: 1px;
    }

    .stall-text {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    /* Matches the conflict card's summary line in `TransferDialog`: the verdict
       reads at the body size, in ordinary primary text on the tint. */
    .stall-reason {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .stall-detail {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
