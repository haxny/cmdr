<!--
  Test-only toast content: shows the `toastId` the toast frame handed it, plus a
  close button wired the way every self-closing toast body is, so a test can see
  whether a component toast raised without `props` still knows its own id. It also
  shows the kind of the `report` incident the frame handed it, if any.
-->
<script lang="ts">
    import { dismissToast } from './toast-store.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { ReportIncident } from '$lib/error-reporter/report-incident'

    interface Props {
        toastId: string
        label?: string
        report?: ReportIncident
    }

    const { toastId, label = '', report }: Props = $props()
</script>

<p data-test="toast-id">{toastId}</p>
<p data-test="toast-label">{label}</p>
<p data-test="toast-report">{report?.kind ?? ''}</p>
<button
    type="button"
    onclick={() => {
        dismissToast(toastId)
    }}>{tString('ui.toast.dismissAria')}</button
>
