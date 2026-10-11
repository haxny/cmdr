<!--
  Test-only toast content: shows the kind of the `report` incident the toast frame
  handed it, so a test can see that a component toast gets its report offer.
-->
<script lang="ts">
    import type { ReportIncident } from '$lib/error-reporter/report-incident'

    interface Props {
        toastId: string
        report?: ReportIncident
    }

    const { report }: Props = $props()
</script>

<p data-test="toast-report">{report?.kind ?? ''}</p>
