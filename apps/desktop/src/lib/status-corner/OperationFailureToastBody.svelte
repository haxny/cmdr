<script lang="ts">
    /**
     * The shared body of the two failure notices (`OperationFailedToastContent`,
     * `OperationFailuresToastContent`): a title, whatever the notice adds under it, and the
     * actions: "Send error report…" (the incident the toast frame handed down) and "Show in
     * operation queue". Both dismiss the toast: the report dialog or the queue window then
     * owns this conversation in full, and leaving the toast up behind it would be two voices
     * saying the same thing. The severity glyph is the toast frame's own `error`
     * level icon, so the title carries none.
     */
    import type { Snippet } from 'svelte'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { dismissToast } from '#lib/ui/toast/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { openQueueWindow } from '#lib/file-operations/queue/queue-window.js'
    import ReportOffer from '#lib/error-reporter/ReportOffer.svelte'
    import type { ReportIncident } from '#lib/error-reporter/report-incident.js'

    interface Props {
        /** Dedup id of this toast; lets the notice get out of its own way. */
        toastId: string
        title: string
        /** What "Send error report…" reports; the frame hands it to the notice as `report`. */
        report?: ReportIncident
        /** What the notice adds under the title (one failure's reason); the summary adds nothing. */
        children?: Snippet
    }

    const { toastId, title, report, children }: Props = $props()

    function showInQueue(): void {
        void openQueueWindow()
        dismissToast(toastId)
    }
</script>

<div class="toast-body">
    <span class="title">{title}</span>
    {@render children?.()}
    <ToastActions>
        {#if report}
            <ReportOffer
                incident={report}
                size="mini"
                onOpen={() => {
                    dismissToast(toastId)
                }}
            />
        {/if}
        <Button size="mini" variant="secondary" onclick={showInQueue}>{tString('queue.failureToast.action')}</Button>
    </ToastActions>
</div>

<style>
    .toast-body {
        font-size: var(--font-size-sm);
    }

    .title {
        color: var(--color-text-primary);
        font-weight: 600;
    }
</style>
