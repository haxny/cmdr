<script lang="ts">
    /**
     * The note after a delete or trash found items already gone
     * (`already-gone-toast.ts`). The frame's × closes this one; "Never show this
     * again" also turns `fileOperations.showAlreadyGoneToast` off, which
     * Settings > Advanced turns back on.
     */
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { dismissToast } from '#lib/ui/toast/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { setSetting } from '#lib/settings/index.js'

    interface Props {
        /** Dedup id of this toast, so the button can close it. */
        toastId: string
        /** The composed, localized sentence. */
        message: string
    }

    const { toastId, message }: Props = $props()

    function handleNeverShowAgain(): void {
        setSetting('fileOperations.showAlreadyGoneToast', false)
        dismissToast(toastId)
    }
</script>

<div class="toast-body">
    <span class="message">{message}</span>
    <ToastActions>
        <Button size="mini" variant="secondary" onclick={handleNeverShowAgain}>
            {tString('fileOperations.alreadyGone.neverShowAgain')}
        </Button>
    </ToastActions>
</div>

<style>
    .toast-body {
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
    }
</style>
