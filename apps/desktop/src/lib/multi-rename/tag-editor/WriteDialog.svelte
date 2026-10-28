<script lang="ts">
    /**
     * The Tags window's Write dialog, over the panes: the Apply panel (`ApplyPanel`) with every step it has, from
     * "Finishing the last change…" through the change list, the write, the would-drop choice and the outcome, to its
     * worded refusals. ⌘S opens it listing the changes (`mode: 'write'`); ⌘Z opens it on the undo question
     * (`mode: 'undo'`), and a "no" closes it again. ⌘⏎ (`multiRename.rename`) presses the panel's primary button. It
     * can't be closed while a write or an undo is on its way; when the window was asked to close meanwhile
     * (`closeWaits`) it says to wait until it's done.
     */
    import { onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import ApplyPanel from './ApplyPanel.svelte'
    import type { TagEditorState } from './tag-editor-state.svelte'

    interface Props {
        tags: TagEditorState
        mode: 'write' | 'undo'
        /** The window was asked to close during the write: say to wait until it's done. */
        closeWaits?: boolean
        onWriting?: () => void
        /** The dialog closed; `notWritten`: how many files the write it showed left unwritten. */
        onClose: (notWritten: number) => void
    }

    const { tags, mode, closeWaits = false, onWriting, onClose }: Props = $props()

    let panel = $state<{ start: () => Promise<void>; undo: () => Promise<boolean>; notWritten: () => number }>()
    let busy = $state(false)

    onMount(() => {
        if (mode === 'undo') {
            void panel?.undo().then((undid) => {
                if (!undid) onClose(0)
            })
        } else void panel?.start()
    })

    function close(): void {
        onClose(panel?.notWritten() ?? 0)
    }

    /**
     * ⌘⏎ presses the primary button. It reads `multiRename.rename` on purpose: the Apply panel's buttons show that
     * command's key (`keyHint`), the same Start key as in the Multi-Rename sheet, so the hint and the key stay one.
     */
    function onkeydown(e: KeyboardEvent): void {
        // Only while writing does Escape get here (no `onclose`): it waits, and never reaches macOS's full screen.
        if (e.key === 'Escape') e.preventDefault()
        if (e.isComposing || !eventMatchesCommand(e, 'multiRename.rename')) return
        claimKey(e)
        void panel?.start()
    }
</script>

<ModalDialog
    titleId="tag-write-title"
    containerStyle="width: 520px; max-width: 90vw"
    growDownward
    onclose={busy || tags.writing ? undefined : close}
    {onkeydown}
>
    {#snippet title()}{tString('multiRename.tags.dialog.write')}{/snippet}
    {#if closeWaits && tags.writing}
        <p class="close-waits" role="status">{tString('multiRename.tags.write.closeWaits')}</p>
    {/if}
    <ApplyPanel bind:this={panel} {tags} {onWriting} onBusy={(now: boolean) => { busy = now }} />
</ModalDialog>

<style>
    .close-waits {
        margin: 0 0 var(--spacing-xs);
        color: var(--color-warning-text);
    }
</style>
