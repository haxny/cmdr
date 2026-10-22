<script lang="ts">
    /**
     * The toast shown after a crash report auto-sends, and the one place someone is offered the
     * chance to send the log from that session too.
     *
     * The offer lives here rather than in the settings default on purpose. A crash report is
     * narrow and stack-shaped, which is why `updates.crashReports` is on by default; an error
     * report is an unbounded log bundle, which is why `updates.errorReports` is off. This button
     * is per-report consent at the one moment it means something, and it changes neither setting.
     * ❌ Never send the log without the press. `#lib/crash-reporter/DETAILS.md` § The three report
     * consents.
     */
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { openSettingsWindow } from '#lib/settings/settings-window.js'
    import { t, tString } from '#lib/intl/messages.svelte.js'
    import { crashSentToastKey } from './crash-copy'
    import { sendCrashLogReport } from '#lib/tauri-commands/error-reporter.js'
    import { errorReportSendFailureOf, errorReportSendReason } from '#lib/error-reporter/error-report-send-error.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import type { CrashReport } from '#lib/tauri-commands/index.js'

    const log = getAppLogger('crashReporter')

    // The report that was just auto-sent. It decides whether this toast may say "crash":
    // a panic the app walked away from didn't crash it. `./crash-copy.ts`.
    const { report }: { report: CrashReport } = $props()

    /** `null` until someone presses the button; then the id of the log report that landed. */
    let sentLogId = $state<string | null>(null)
    let sendingLog = $state(false)
    /** Why the attempt didn't land, worded from the catalog, or `null` before an attempt did that. */
    let logNotSentReason = $state<string | null>(null)

    // The offer is a one-shot. It goes away once the log lands, and also once a send DOESN'T: a
    // toast carrying an invitation, a failure, and a retry all at once reads as three things
    // competing, and the report this is about has already been sent either way. Someone who wants
    // to try again still has Help > Send error report.
    const offerOpen = $derived(!sentLogId && !logNotSentReason)

    function handleOpenSettings() {
        dismissToast('crash-report-sent')
        void openSettingsWindow('crash-toast')
    }

    async function handleSendLog() {
        if (sendingLog || !offerOpen) return
        sendingLog = true
        logNotSentReason = null
        try {
            // The crash's own timestamp, so the bundle is scoped around when the app died rather
            // than around this launch. `report.shortId` ties the two reports together for triage.
            const { id } = await sendCrashLogReport(report.shortId ?? '', report.timestamp)
            sentLogId = id
            log.info('Crash log report sent as {id}', { id })
        } catch (e) {
            // No network or a server having a bad moment isn't worth an error line: it would try to
            // auto-report through the server that just didn't answer.
            log.warn('Sending the crash log report returned an error: {error}', { error: e })
            logNotSentReason = errorReportSendReason(errorReportSendFailureOf(e))
        } finally {
            sendingLog = false
        }
    }
</script>

<div class="content">
    <span class="message">
        {#if sentLogId}
            {tString('crashReporter.sentToast.logSent', { id: sentLogId })}
        {:else}
            {tString(crashSentToastKey(report))}
        {/if}
    </span>
    {#if offerOpen}
        <p class="hint">{t('crashReporter.sentToast.sendLogHint')}</p>
    {/if}
    {#if logNotSentReason}
        <p class="not-sent" role="alert">
            {tString('crashReporter.sentToast.logNotSent', { reason: logNotSentReason })}
        </p>
    {/if}
    <ToastActions>
        {#if offerOpen}
            <Button size="mini" variant="secondary" disabled={sendingLog} onclick={handleSendLog}>
                {sendingLog
                    ? tString('crashReporter.dialog.sending')
                    : tString('crashReporter.sentToast.sendLog')}
            </Button>
        {/if}
        <Button size="mini" variant="secondary" onclick={handleOpenSettings}
            >{tString('crashReporter.sentToast.changeSettings')}</Button
        >
    </ToastActions>
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
    }

    .hint {
        margin: var(--spacing-sm) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
    }

    .not-sent {
        margin: var(--spacing-sm) 0 0;
        color: var(--color-error);
        font-size: var(--font-size-xs);
    }
</style>
