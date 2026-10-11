<script lang="ts">
    /**
     * What a burst of failures collapses into: one notice with the count.
     *
     * It reads the count off the main window's store rather than taking it as a
     * prop, for one mechanical reason: the toast store's dedup path replaces a
     * toast's content and level but NOT its props, so a prop-carried count
     * would freeze at whatever the fourth failure saw. Reading live also keeps
     * it honest while the user clears rows in the queue window.
     */
    import OperationFailureToastBody from './OperationFailureToastBody.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getMainWindowOperationRows } from '$lib/file-operations/queue/main-window-operations.svelte'
    import type { ReportIncident } from '$lib/error-reporter/report-incident'

    interface Props {
        /** Dedup id of this toast; lets the component get out of its own way. */
        toastId: string
        /** The toast's report offer, from the frame. */
        report?: ReportIncident
    }

    const { toastId, report }: Props = $props()

    const count = $derived(getMainWindowOperationRows().filter((row) => row.snapshot.status === 'failed').length)
    const title = $derived(tString('queue.failureToast.summary', { count, countText: formatInteger(count) }))
</script>

<!-- The raise's count froze at the fourth failure (props don't follow a dedup), so the
     report takes the live one, the same count the title shows. -->
<OperationFailureToastBody
    {toastId}
    {title}
    report={report?.kind === 'operationsFailed' ? { kind: 'operationsFailed', count } : report}
/>
