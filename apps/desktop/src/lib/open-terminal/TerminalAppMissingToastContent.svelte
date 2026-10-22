<script lang="ts">
    /**
     * The toast for a chosen terminal app that has been uninstalled: Cmdr opened
     * Terminal.app instead, and the caller has already reset the setting to it so
     * this can't repeat. The button leads to the row, in case the user has since
     * installed something else.
     *
     * `appName` is `null` when Cmdr carries no name for the stored choice (a
     * bundle id this version doesn't know). The unnamed wording covers that; ❌
     * never show the bundle id, which reads like a name and isn't one.
     */
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { openSettingsToTerminalApp } from './terminal-app-setting'

    interface Props {
        toastId: string
        appName: string | null
    }

    const { toastId, appName }: Props = $props()

    const message = $derived(
        appName === null
            ? tString('commands.handler.openTerminalHere.appMissingUnnamed')
            : tString('commands.handler.openTerminalHere.appMissing', { app: appName }),
    )

    async function handleOpenSettings(): Promise<void> {
        dismissToast(toastId)
        await openSettingsToTerminalApp()
    }
</script>

<div class="content">
    <span class="message">{message}</span>
    <ToastActions>
        <Button size="mini" variant="secondary" onclick={() => void handleOpenSettings()}
            >{tString('commands.handler.openTerminalHere.openSettings')}</Button
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
</style>
