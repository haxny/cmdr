<script lang="ts">
    /**
     * Get info's answer when the user has told macOS not to let Cmdr control Finder:
     * what to switch, and a button that opens the pane it lives in.
     *
     * Without this, every later Get info press did nothing at all, since macOS drops
     * the ask without a word (`src-tauri/src/file_system/get_info.rs`).
     */
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { openAutomationSettings } from '#lib/tauri-commands/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'
</script>

<div class="content">
    <span class="message">{tString('commands.handler.getInfo.automationOff')}</span>
    <ToastActions>
        <Button variant="primary" size="mini" onclick={() => void openAutomationSettings()}>
            {tString('commands.handler.getInfo.openAutomationSettings')}
        </Button>
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
