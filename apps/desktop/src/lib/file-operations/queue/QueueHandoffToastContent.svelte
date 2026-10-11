<script lang="ts">
    /**
     * The toast a job sent to the background leaves behind (setup-dialog F2, the
     * progress dialog's Background / Queue, an auto-queue): where it went, and the
     * one way to the queue window, which backgrounding never opens by itself. The
     * action dismisses the toast, as the failure notice's does: the window then
     * says it all.
     */
    import Button from '$lib/ui/Button.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'
    import { openQueueWindow } from '$lib/file-operations/queue/queue-window'

    interface Props {
        /** Dedup id of this toast; lets the action get out of its own way. */
        toastId: string
        /** The already-resolved sentence saying where the job went. */
        message: string
    }

    const { toastId, message }: Props = $props()

    function showQueue(): void {
        void openQueueWindow()
        dismissToast(toastId)
    }
</script>

<div class="toast-body">
    <span>{message}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={showQueue}>{tString('queue.handoffToast.action')}</Button>
    </div>
</div>

<style>
    .toast-body {
        font-size: var(--font-size-sm);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        margin-top: var(--spacing-sm);
    }
</style>
