<script lang="ts">
    /**
     * What a window shows when its page threw while rendering, in place of SvelteKit's bare
     * "500 / Internal Error". `hooks.client.ts` has already logged the throw (so an opted-in
     * auto-send ships it) and handed over its `Name: message` as `page.error.detail`.
     *
     * Only the root layout sits above this page, so in the main window nothing else hosts the
     * report dialog: this page hosts it, with the toasts its send confirms through. Other windows
     * ask the main window, as everywhere else.
     */
    import { onMount, tick } from 'svelte'
    import { page } from '$app/state'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import Button from '#lib/ui/Button.svelte'
    import ToastContainer from '#lib/ui/toast/ToastContainer.svelte'
    import ErrorReportDialog from '#lib/error-reporter/ErrorReportDialog.svelte'
    import ReportOffer from '#lib/error-reporter/ReportOffer.svelte'
    import { errorReportFlow, hostErrorReportDialog } from '#lib/error-reporter/error-report-flow.svelte.js'
    import { tString } from '#lib/intl/messages.svelte.js'

    const detail = $derived(page.error?.detail ?? null)
    let isMainWindow = $state(false)
    let headingRef: HTMLHeadingElement | undefined

    onMount(() => {
        // Focus lands on the heading, so a screen reader says what happened and Tab reaches the buttons.
        void tick().then(() => headingRef?.focus())
        isMainWindow = getCurrentWindow().label === 'main'
        if (!isMainWindow) return
        return hostErrorReportDialog()
    })

    function reload(): void {
        location.reload()
    }
</script>

<main class="page-crash">
    <h1 bind:this={headingRef} tabindex="-1">{tString('errorReporter.pageCrash.title')}</h1>
    <p class="body">{tString('errorReporter.pageCrash.body')}</p>
    {#if detail}
        <section class="details" aria-labelledby="page-crash-details-label">
            <h2 id="page-crash-details-label">{tString('errorReporter.pageCrash.detailsLabel')}</h2>
            <pre>{detail}</pre>
        </section>
    {/if}
    <div class="actions">
        <ReportOffer incident={{ kind: 'pageCrash', detail }} />
        <Button variant="primary" onclick={reload}>{tString('errorReporter.pageCrash.reload')}</Button>
    </div>
</main>

{#if isMainWindow}
    <ToastContainer />
    {#if errorReportFlow.open}
        <ErrorReportDialog />
    {/if}
{/if}

<style>
    .page-crash {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
        max-width: 560px;
        margin: 0 auto;
        padding: var(--spacing-2xl) var(--spacing-xl);
        color: var(--color-text-primary);
    }

    h1 {
        margin: 0;
        font-size: var(--font-size-xl);
        outline: none;
    }

    .body {
        margin: 0;
        color: var(--color-text-secondary);
    }

    h2 {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-sm);
        font-weight: normal;
        color: var(--color-text-secondary);
    }

    pre {
        margin: 0;
        padding: var(--spacing-sm) var(--spacing-md);
        border-radius: var(--radius-md);
        background: var(--color-bg-secondary);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        white-space: pre-wrap;
        overflow-wrap: anywhere;
        user-select: text;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
    }
</style>
