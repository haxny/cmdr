<script lang="ts">
    /**
     * The setup dialogs' Background / Queue button (copy, move, compress, and
     * trash): confirm, and start the operation with no progress dialog (#381).
     * F2 in the dialog presses it (`start-in-background-key.ts`). One button, two words, by the same rule as the
     * progress dialog's: "Queue" while other work is live, "Background" when the
     * queue is empty. The F2 chip inside it is how a mouse user learns the key.
     * `transfer/DETAILS.md` § "Starting in the background".
     */
    import Button from '$lib/ui/Button.svelte'
    import KeyHint from '$lib/ui/KeyHint.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getMainWindowOperationRows } from '$lib/file-operations/queue/main-window-operations.svelte'
    import { hasOtherQueuedWork } from '$lib/file-operations/queue/queue-backlog'

    interface Props {
        onclick: () => void
        disabled?: boolean
    }

    const { onclick, disabled = false }: Props = $props()

    /** No operation of its own yet, so nothing to exclude. */
    const queueHasOtherWork = $derived(hasOtherQueuedWork(getMainWindowOperationRows(), null))
</script>

<Button
    variant="secondary"
    {onclick}
    {disabled}
    aria-keyshortcuts="F2"
    tooltipContent={tString('fileOperations.backgroundStart.tooltip')}
    aria-label={queueHasOtherWork
        ? tString('fileOperations.backgroundStart.queueAria')
        : tString('fileOperations.backgroundStart.backgroundAria')}
>
    {queueHasOtherWork
        ? tString('fileOperations.backgroundStart.queue')
        : tString('fileOperations.backgroundStart.background')}<KeyHint key="F2" />
</Button>
