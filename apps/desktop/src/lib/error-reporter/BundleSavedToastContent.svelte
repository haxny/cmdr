<script lang="ts">
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { showInFinder } from '#lib/tauri-commands/index.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'

    interface Props {
        toastId: string
        /** Where the debug bundle landed. */
        path: string
    }

    const { toastId, path }: Props = $props()

    function handleReveal() {
        if (path) {
            void showInFinder(path)
        }
    }

    function handleDismiss() {
        dismissToast(toastId)
    }
</script>

<div class="content">
    <span class="message">{tString('errorReporter.bundleSavedToast.message')}</span>
    <span class="path" use:tooltip={{ text: path, overflowOnly: true }}>{path}</span>
    <ToastActions>
        <Button size="mini" variant="secondary" onclick={handleDismiss}
            >{tString('errorReporter.bundleSavedToast.dismiss')}</Button
        >
        <Button size="mini" variant="primary" onclick={handleReveal}
            >{tString('errorReporter.bundleSavedToast.reveal')}</Button
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

    .path {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        max-width: 320px;
    }
</style>
