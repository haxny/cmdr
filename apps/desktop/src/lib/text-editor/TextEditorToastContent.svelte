<script lang="ts">
    /**
     * The one body every text-editor toast uses: a sentence the caller has already
     * resolved, an optional Dismiss, and "Open settings", which leads to the row
     * that picks the app.
     *
     * The message arrives resolved because each toast words a different report (the
     * app the file opened in, or nothing to name), while the buttons stay the same.
     */
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { openSettingsToTextEditor } from './text-editor-setting'

    interface Props {
        toastId: string
        message: string
        /** Adds Dismiss beside "Open settings". Without it, the toast's own close button is the way out. */
        showDismiss?: boolean
    }

    const { toastId, message, showDismiss = false }: Props = $props()

    function handleDismiss(): void {
        dismissToast(toastId)
    }

    async function handleOpenSettings(): Promise<void> {
        dismissToast(toastId)
        await openSettingsToTextEditor()
    }
</script>

<div class="content">
    <span class="message">{message}</span>
    <ToastActions>
        {#if showDismiss}
            <Button size="mini" variant="secondary" onclick={handleDismiss}
                >{tString('fileExplorer.edit.dismiss')}</Button
            >
        {/if}
        <Button
            size="mini"
            variant={showDismiss ? 'primary' : 'secondary'}
            onclick={() => void handleOpenSettings()}>{tString('fileExplorer.edit.openSettings')}</Button
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
