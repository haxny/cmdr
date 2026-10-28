<script lang="ts">
    /**
     * "Quit while operations are running?" — the prompt the backend raises when
     * an exit would interrupt a copy, move, delete, trash, or zip edit, or (fork)
     * lose a Tags window's unwritten edits or cut a tag write short (`tags`).
     *
     * Prop-driven and inert: it renders what it's handed and calls back. The
     * countdown it shows is Rust's, mirrored (`quit-prompt.svelte.ts`), and
     * `topmost` puts it over any dialog already open — including a modal
     * conflict prompt, which is exactly the state a user reaches for ⌘Q from.
     */
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { operationTypeIcon } from '#lib/file-operations/queue/operation-icon.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { OperationSnapshot, TagsAtQuit } from '#lib/tauri-commands/index.js'

    interface Props {
        /** The operations holding the quit, in the backend's registration order. */
        operations: OperationSnapshot[]
        /** What the Tags windows would lose, or are writing (fork). */
        tags?: TagsAtQuit
        /** Whether the backend quits on its own when the seconds run out; not for unwritten tag edits alone (fork). */
        countsDown?: boolean
        /** Seconds left on the backend's clock. Display only. */
        secondsLeft: number
        /** Quit now, skipping the rest of the countdown. */
        onQuit: () => void
        /** Call the quit off entirely. Also what Escape and the × do. */
        onKeepWorking: () => void
    }

    const {
        operations,
        tags = { unwritten: 0, writing: false },
        countsDown = true,
        secondsLeft,
        onQuit,
        onKeepWorking,
    }: Props = $props()

    const titleText = $derived.by(() => {
        if (operations.length > 0) {
            return tString('main.quit.title', { count: operations.length, countText: formatInteger(operations.length) })
        }
        if (tags.writing) return tString('multiRename.tags.quit.writingTitle')
        return tString('multiRename.tags.quit.title', {
            count: tags.unwritten,
            countText: formatInteger(tags.unwritten),
        })
    })
    const countdown = $derived(
        tString('main.quit.countdown', {
            seconds: secondsLeft,
            secondsText: formatInteger(secondsLeft),
        }),
    )

    /** Last path segment, the same compact summary the queue window's rows show. */
    function basename(path: string | null): string {
        if (!path) return ''
        const trimmed = path.replace(/\/+$/, '')
        const idx = trimmed.lastIndexOf('/')
        return idx >= 0 ? trimmed.slice(idx + 1) : trimmed
    }
</script>

<ModalDialog
    titleId="quit-confirmation-title"
    ariaDescribedby="quit-confirmation-body"
    role="alertdialog"
    dialogId="quit-confirmation"
    topmost
    blur
    onclose={onKeepWorking}
    containerStyle="min-width: 460px; max-width: 560px"
>
    {#snippet title()}{titleText}{/snippet}

    <div id="quit-confirmation-body">
        {#if operations.length > 0}
            <p class="body">{tString('main.quit.body')}</p>
        {/if}
        {#if tags.writing}
            <p class="body">{tString('multiRename.tags.quit.writing')}</p>
        {/if}
        {#if tags.unwritten > 0}
            <p class="body">
                {tString('multiRename.tags.quit.unwritten', {
                    count: tags.unwritten,
                    countText: formatInteger(tags.unwritten),
                })}
            </p>
        {/if}
    </div>

    {#if operations.length > 0}
        <h3 class="operations-heading">{tString('main.quit.operationsHeading')}</h3>
        <ul class="operations" aria-label={tString('queue.list.aria')}>
            {#each operations as operation (operation.operationId)}
                <li class="operation">
                    <span class="type" aria-hidden="true">
                        <Icon name={operationTypeIcon(operation.operationType)} size={14} />
                    </span>
                    <span class="op-label">{tString('queue.row.label', { type: operation.operationType })}</span>
                    {#if operation.source}
                        <span class="path" use:tooltip={{ text: operation.source, overflowOnly: true }}
                            >{basename(operation.source)}</span
                        >
                    {/if}
                    {#if operation.destination}
                        <span class="arrow" aria-hidden="true">&#x2192;</span>
                        <span class="path" use:tooltip={{ text: operation.destination, overflowOnly: true }}
                            >{basename(operation.destination)}</span
                        >
                    {/if}
                </li>
            {/each}
        </ul>
    {/if}

    <!-- `polite`, not `assertive`: a screen reader shouldn't interrupt the user
         once a second. The label names what the number measures. -->
    {#if countsDown}
        <p class="countdown" aria-live="polite" aria-label={tString('main.quit.countdownAria')}>{countdown}</p>
    {:else}
        <p class="countdown">{tString('multiRename.tags.quit.waits')}</p>
    {/if}

    {#snippet footer()}
        <Button variant="secondary" autoFocus onclick={onKeepWorking}>{tString('main.quit.keepWorking')}</Button>
        <Button variant="danger" onclick={onQuit}>{tString('main.quit.quitNow')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .body {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: var(--font-line-height-prose);
    }

    .body + .body {
        margin-top: var(--spacing-xs);
    }

    .operations-heading {
        margin: var(--spacing-lg) 0 var(--spacing-xs);
        font-size: var(--font-size-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--color-text-tertiary);
    }

    /* Capped and scrollable: a batch session can have a dozen operations, and the
       countdown below must stay on screen no matter how many. */
    .operations {
        margin: 0;
        padding: 0;
        list-style: none;
        max-height: 132px;
        overflow-y: auto;
    }

    .operation {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        min-width: 0;
        padding: var(--spacing-xxs) 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .type {
        display: flex;
        flex: none;
        color: var(--color-text-tertiary);
    }

    .op-label {
        flex: none;
        color: var(--color-text-primary);
    }

    .path {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .arrow {
        flex: none;
        color: var(--color-text-tertiary);
    }

    .countdown {
        margin: var(--spacing-lg) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        line-height: var(--font-line-height-prose);
    }
</style>
