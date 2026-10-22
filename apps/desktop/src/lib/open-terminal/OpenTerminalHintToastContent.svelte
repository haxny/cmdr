<script lang="ts">
    /**
     * The one-time INFO toast raised the first time "Open terminal here" runs on
     * a Mac carrying more than one terminal app. It says what just happened and
     * offers the row that picks a different app.
     *
     * Persistent on purpose: it's the only place this setting is ever advertised,
     * and it shows exactly once (the caller spends
     * `behavior.openTerminalHereToastSeen` when it raises this). A four-second
     * transient toast would be a coin flip on whether anyone read it.
     */
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { openSettingsToTerminalApp } from './terminal-app-setting'

    interface Props {
        toastId: string
    }

    const { toastId }: Props = $props()

    function handleDismiss(): void {
        dismissToast(toastId)
    }

    async function handleOpenSettings(): Promise<void> {
        dismissToast(toastId)
        await openSettingsToTerminalApp()
    }
</script>

<div class="content">
    <span class="message">{tString('commands.handler.openTerminalHere.hint')}</span>
    <ToastActions>
        <Button size="mini" variant="secondary" onclick={handleDismiss}
            >{tString('commands.handler.openTerminalHere.dismiss')}</Button
        >
        <Button size="mini" variant="primary" onclick={() => void handleOpenSettings()}
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
