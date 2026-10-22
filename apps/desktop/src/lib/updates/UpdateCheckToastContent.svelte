<script lang="ts">
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { updateState } from './update-state.svelte'
    import { describeUpdateFailure, formatUpdateStatus, updateFailureOffersReport } from './update-status-text'
    import ReportOffer from '#lib/error-reporter/ReportOffer.svelte'

    const statusText = $derived(formatUpdateStatus(updateState))
    const failureText = $derived(updateState.failure === null ? null : describeUpdateFailure(updateState.failure))
    const offersReport = $derived(updateState.failure !== null && updateFailureOffersReport(updateState.failure))
</script>

<div class="content">
    {#if failureText !== null}
        <span class="message">{failureText}</span>
        {#if offersReport}
            <ToastActions>
                <!-- The note is the sentence the toast showed; the raw detail is already in the log the report bundles. -->
                <ReportOffer incident={{ kind: 'message', from: 'updateFailure', text: failureText }} size="mini" />
            </ToastActions>
        {/if}
    {:else}
        <span class="message">{statusText}</span>
    {/if}
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }
</style>
