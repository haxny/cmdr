<script lang="ts">
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { openSettingsWindow } from '#lib/settings/settings-window.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { pastedAsFileMessage } from './pane/paste-clipboard-as-file-message'
    import type { PastedClipboardFile } from '#lib/tauri-commands/index.js'

    interface Props {
        filename: string
        kind: PastedClipboardFile['kind']
        /** Injected by the toast store so the Settings button can self-dismiss. */
        toastId: string
    }

    const { filename, kind, toastId }: Props = $props()

    function handleOpenSettings() {
        dismissToast(toastId)
        void openSettingsWindow('paste-toast', ['Behavior', 'Navigation & file ops'])
    }
</script>

<div class="content">
    <span class="message">{pastedAsFileMessage(kind, filename)}</span>
    <ToastActions>
        <Button size="mini" variant="secondary" onclick={handleOpenSettings}
            >{tString('fileExplorer.clipboard.pastedAsFileSettings')}</Button
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
