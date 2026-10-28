<script lang="ts">
    /**
     * The box F2 lays over a pane row, the way the file panes' inline rename does (`InlineRenameEditor`): it takes
     * focus with its text selected, Enter (or ⌘⏎) commits, Esc cancels, and leaving it (a click elsewhere) commits.
     * Like `EditableCell` it sends nothing when the text is unchanged, so Enter on an untouched «multiple values» box
     * never writes. Multiline for Comment and Description: ⌥Enter adds a line. Every key it acts on is claimed, so
     * Esc never reaches the window's `tags.close`.
     */
    import { onMount, untrack } from 'svelte'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'

    interface Props {
        id?: string
        /** The text the row holds; the box starts with it. */
        value: string
        ariaLabel: string
        multiline?: boolean
        invalid?: boolean
        describedBy?: string
        /** Where the box sits over its row (`top`/`left`/`width`/`height`), set by the pane. */
        style?: string
        /** Called with the typed text, only when it differs from `value`. */
        onCommit: (text: string) => void | Promise<void>
        /**
         * The box is done (committed or cancelled); the pane drops it. `refocus`: focus went nowhere else (a key closed
         * it, or a click on no control), so the pane takes it back.
         */
        onClose: (refocus: boolean) => void
    }

    const { id, value, ariaLabel, multiline = false, invalid = false, describedBy, style, onCommit, onClose }: Props = $props()

    const log = getAppLogger('tagEditor')

    let draft = $state(untrack(() => value))
    let box = $state<HTMLInputElement | HTMLTextAreaElement>()
    let done = false

    onMount(() => {
        box?.focus()
        box?.select()
    })

    function finish(commit: boolean, refocus = true): void {
        if (done) return
        done = true
        if (commit && draft !== value) {
            // A refused edit is the state's to word; here it is only logged, by kind, never the text.
            void new Promise<void>((resolve) => {
                resolve(onCommit(draft))
            }).catch((e: unknown) => {
                log.warn("a tag edit didn't go through: {reason}", { reason: e instanceof Error ? e.name : typeof e })
            })
        }
        onClose(refocus)
    }

    /** ⌥Enter in a multiline box: a new line at the caret, replacing any selected text. */
    function insertLine(el: HTMLTextAreaElement): void {
        el.setRangeText('\n', el.selectionStart, el.selectionEnd, 'end')
        draft = el.value
    }

    /** What an Enter means here: ⌥Enter a new line (multiline only), Enter or ⌘⏎ the commit. */
    function enterKind(e: KeyboardEvent): 'line' | 'commit' | null {
        const lineEnter = e.key === 'Enter' && e.altKey && !e.shiftKey && !e.metaKey && !e.ctrlKey
        if (multiline && lineEnter) return 'line'
        const plainEnter = e.key === 'Enter' && !e.altKey && !e.shiftKey && !e.metaKey && !e.ctrlKey
        return plainEnter || eventMatchesCommand(e, 'multiRename.rename') ? 'commit' : null
    }

    function onkeydown(e: KeyboardEvent): void {
        if (e.isComposing) return
        const enter = enterKind(e)
        if (enter === 'line' && box instanceof HTMLTextAreaElement) {
            claimKey(e)
            insertLine(box)
        } else if (enter === 'commit') {
            claimKey(e)
            finish(true)
        } else if (e.key === 'Escape') {
            claimKey(e)
            finish(false)
        }
    }
</script>

{#if multiline}
    <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- laid over a pane row like the inline rename field: it takes the row's font and box, which the framed `TextArea` (its padding, radius, ring) can't. -->
    <textarea
        bind:this={box}
        bind:value={draft}
        {id}
        class="tag-value-editor multiline"
        aria-label={ariaLabel}
        aria-invalid={invalid ? 'true' : undefined}
        aria-describedby={describedBy}
        rows={4}
        {style}
        {onkeydown}
        onblur={(e: FocusEvent) => { finish(true, e.relatedTarget === null) }}
    ></textarea>
{:else}
    <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- laid over a pane row like the inline rename field (`InlineRenameEditor`): it takes the row's height and font, which the framed `TextInput`'s padding would break. -->
    <input
        bind:this={box}
        bind:value={draft}
        {id}
        type="text"
        class="tag-value-editor"
        aria-label={ariaLabel}
        aria-invalid={invalid ? 'true' : undefined}
        aria-describedby={describedBy}
        {style}
        {onkeydown}
        onblur={(e: FocusEvent) => { finish(true, e.relatedTarget === null) }}
    />
{/if}

<style>
    /* The inline rename field's look (`InlineRenameEditor`), in the accent rather than a severity color. */
    .tag-value-editor {
        position: absolute;
        z-index: 1;
        box-sizing: border-box;
        margin: 0;
        padding: 0 var(--spacing-xxs);
        font: inherit;
        color: var(--color-text-primary);
        background: var(--color-bg-primary);
        border: 2px solid var(--color-accent);
        border-radius: var(--radius-sm);
        outline: none;
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }

    .tag-value-editor.multiline {
        padding: var(--spacing-xxs);
        line-height: var(--font-line-height-normal);
        resize: none;
    }

    .tag-value-editor[aria-invalid='true'] {
        border-color: var(--color-error);
    }
</style>
