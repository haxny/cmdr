<script lang="ts">
    /**
     * "Connect directly"'s answer when something on this Mac refused Cmdr's own
     * connection to a server the Mac itself reaches (`blockedByThisMac`): what to
     * switch, a button that opens it, and the report offer the toast frame hands down.
     *
     * The fix in ERR-XGS9X was a switch in System Settings, so a sentence alone
     * would leave the person hunting for the pane.
     */
    import Button from '#lib/ui/Button.svelte'
    import ReportOffer from '#lib/error-reporter/ReportOffer.svelte'
    import type { ReportIncident } from '#lib/error-reporter/report-incident.js'
    import { dismissToast } from '#lib/ui/toast/index.js'
    import { openLocalNetworkSettings } from '#lib/tauri-commands/index.js'
    import { directConnectionUnavailableMessage, openLocalNetworkSettingsLabel } from './upgrade-messages'

    interface Props {
        /** The server's friendly name, which the sentence names. */
        server: string
        toastId: string
        /** The toast's report offer, from the frame (`direct-connect.ts` raises it). */
        report?: ReportIncident
    }

    const { server, toastId, report }: Props = $props()
</script>

<div class="content">
    <span class="message">{directConnectionUnavailableMessage('blockedByThisMac', server)}</span>
    <div class="actions">
        {#if report}
            <ReportOffer
                incident={report}
                size="mini"
                onOpen={() => {
                    dismissToast(toastId)
                }}
            />
        {/if}
        <Button variant="primary" size="mini" onclick={() => void openLocalNetworkSettings()}>
            {openLocalNetworkSettingsLabel()}
        </Button>
    </div>
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
        overflow-wrap: anywhere;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: calc(var(--spacing-xs) + var(--spacing-xxs));
    }
</style>
