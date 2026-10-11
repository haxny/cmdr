<script lang="ts">
    /**
     * The one "report this" action, for every surface where something went wrong or got
     * slow: the toast frame, the transfer error dialog, the error pane, a refused create,
     * a failed queue row, and the slowness notices. It opens the compose dialog pre-filled
     * from the typed incident (`report-incident.ts`), so no surface words its own note or
     * picks its own breadcrumb.
     *
     * A failure reads "Send error report…" as a button. Something slow reads as the
     * lighter "Taking long? Send a report" link, since nothing has gone wrong yet, and
     * records its slowness as soon as it shows, so a report sent later from the Help menu
     * still carries it.
     */
    import { onMount } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { openErrorReportFor } from './error-report-flow.svelte'
    import { recordBreadcrumb } from './breadcrumbs'
    import { slowBreadcrumbFor, type ReportIncident } from './report-incident'

    interface Props {
        incident: ReportIncident
        /** A button, or an inline link for a surface that words its status in a line of text
         *  (the Settings update row). A slow incident is always a link. */
        look?: 'button' | 'link'
        /** The button's size; a link ignores it. */
        size?: 'regular' | 'mini'
        /** After the dialog was asked to open: a toast closes itself here. */
        onOpen?: () => void
    }

    const { incident, look = 'button', size = 'regular', onOpen }: Props = $props()

    onMount(() => {
        if (incident.kind === 'slow') recordBreadcrumb(slowBreadcrumbFor(incident, Date.now()))
    })

    function offer(): void {
        openErrorReportFor(incident)
        onOpen?.()
    }
</script>

{#if incident.kind === 'slow'}
    <span class="link-offer"><LinkButton onclick={offer}>{tString('errorReporter.offer.slow')}</LinkButton></span>
{:else if look === 'link'}
    <span class="link-offer"><LinkButton onclick={offer}>{tString('errorReporter.offer.send')}</LinkButton></span>
{:else}
    <Button variant="secondary" {size} onclick={offer}>{tString('errorReporter.offer.send')}</Button>
{/if}

<style>
    .link-offer {
        font-size: var(--font-size-sm);
    }
</style>
