<script lang="ts">
    /**
     * One state file Cmdr couldn't read and started fresh (#461): what reset, what that means
     * for the person when it's not obvious, and where the old copy went. One component for every
     * `StateFile`, switched on the file, so the family reads alike. The look is the security
     * notices' (`#lib/security-notices/SecurityNoticeToastContent.svelte`).
     */
    import type { StateRecovery } from '#lib/ipc/bindings.js'
    import Button from '#lib/ui/Button.svelte'
    import { dismissToast } from '#lib/ui/toast/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { openSettingsWindow } from '#lib/settings/settings-window.js'
    import { showInFinder } from '#lib/tauri-commands/index.js'

    interface Props {
        /** Dedup id of this toast; lets the notice get out of its own way. */
        toastId: string
        recovery: StateRecovery
    }

    const { toastId, recovery }: Props = $props()

    const log = getAppLogger('stateRecovery')

    const title = $derived.by(() => {
        switch (recovery.file) {
            case 'settings':
                return tString('stateRecovery.settings.title')
            case 'shortcuts':
                return tString('stateRecovery.shortcuts.title')
            case 'license':
                return tString('stateRecovery.license.title')
            case 'app_status':
                return tString('stateRecovery.appStatus.title')
            case 'operation_log':
                return tString('stateRecovery.operationLog.title')
            case 'chat_history':
                return tString('stateRecovery.chatHistory.title')
            case 'favorites':
                return tString('stateRecovery.favorites.title')
            case 'go_to_path_history':
                return tString('stateRecovery.goToPathHistory.title')
            case 'search_history':
                return tString('stateRecovery.searchHistory.title')
            case 'selection_history':
                return tString('stateRecovery.selectionHistory.title')
            case 'multi_rename_presets':
                return tString('stateRecovery.multiRenamePresets.title')
            case 'multi_rename_last_settings':
                return tString('stateRecovery.multiRenameLastSettings.title')
            case 'multi_rename_field_history':
                return tString('stateRecovery.multiRenameFieldHistory.title')
            case 'launch_days':
                return tString('stateRecovery.launchDays.title')
        }
    })

    /** What the person may want to do about it, for the two files where that isn't obvious. */
    const body = $derived.by(() => {
        switch (recovery.file) {
            case 'settings':
                return tString('stateRecovery.settings.body')
            case 'license':
                return tString('stateRecovery.license.body')
            default:
                return null
        }
    })

    /** Where the settings button lands, for the files that have one. */
    const settingsSection = $derived.by((): { section: string[]; label: string } | null => {
        switch (recovery.file) {
            case 'settings':
                return { section: ['Updates & privacy'], label: tString('stateRecovery.settings.openSettings') }
            case 'license':
                return { section: ['License'], label: tString('stateRecovery.license.openSettings') }
            default:
                return null
        }
    })

    async function openSettings(section: string[]): Promise<void> {
        try {
            await openSettingsWindow('state-recovery-toast', section)
        } catch (error) {
            // The toast stays up, so the button is still there to try again.
            log.warn("Couldn't open Settings from a recovery notice: {error}", { error: String(error) })
            return
        }
        dismissToast(toastId)
    }

    async function reveal(path: string): Promise<void> {
        try {
            await showInFinder(path)
        } catch (error) {
            log.warn("Couldn't show the set-aside copy in Finder: {error}", { error: String(error) })
        }
    }
</script>

<div class="notice">
    <span class="notice-headline">{title}</span>
    {#if body}
        <span class="notice-detail">{body}</span>
    {/if}
    {#if recovery.keptCopy}
        <span class="notice-detail notice-path">{tString('stateRecovery.keptCopy', { path: recovery.keptCopy })}</span>
    {/if}
    <div class="notice-actions">
        <Button
            variant="secondary"
            size="mini"
            onclick={() => {
                dismissToast(toastId)
            }}>{tString('stateRecovery.dismiss')}</Button
        >
        {#if recovery.keptCopy}
            {@const keptCopy = recovery.keptCopy}
            <Button variant="secondary" size="mini" onclick={() => void reveal(keptCopy)}>
                {tString('stateRecovery.showInFinder')}
            </Button>
        {/if}
        {#if settingsSection}
            {@const target = settingsSection}
            <Button variant="primary" size="mini" onclick={() => void openSettings(target.section)}>
                {target.label}
            </Button>
        {/if}
    </div>
</div>

<style>
    /* The security notices' look (`SecurityNoticeToastContent.svelte`), so the family reads alike. */
    .notice-headline {
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .notice-detail {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    /* A long path breaks anywhere instead of widening the toast. */
    .notice-path {
        overflow-wrap: anywhere;
        user-select: text;
    }

    .notice-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
