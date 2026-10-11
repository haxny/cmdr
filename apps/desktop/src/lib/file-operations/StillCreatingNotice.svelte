<script lang="ts">
    /**
     * The new-folder / new-file dialogs' notice while a slow volume is still
     * creating the entry (`create-submission.svelte.ts`, phase `stillCreating`).
     * `role="status"` announces it without taking focus from the name field. Its
     * "Taking long? Send a report" link reports the wait, measured from OK.
     */
    import Spinner from '$lib/ui/Spinner.svelte'
    import ReportOffer from '$lib/error-reporter/ReportOffer.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The name being created. */
        name: string
        kind: 'folder' | 'file'
        /** When OK was pressed, in `Date.now()` time. */
        since: number
    }

    const { name, kind, since }: Props = $props()
</script>

<div class="still-creating">
    <Spinner size="sm" />
    <div class="text">
        <p class="message" role="status">{tString('fileOperations.newEntry.stillCreating', { name })}</p>
        <ReportOffer
            incident={{ kind: 'slow', operation: kind === 'folder' ? 'create_folder' : 'create_file', since, waitingOn: null }}
        />
    </div>
</div>

<style>
    .still-creating {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .message {
        margin: 0 0 var(--spacing-xs);
    }
</style>
