<script lang="ts">
    /**
     * The box F2 lays over a pane row, the way the file panes' inline rename does (`InlineRenameEditor`): it takes
     * focus with its text selected, Enter (or ⌘⏎) commits, Esc cancels, and leaving it (a click elsewhere) commits.
     * Like `EditableCell` it sends nothing when the text is unchanged, so Enter on an untouched «multiple values» box
     * never writes. Multiline for Comment and Description: ⌥Enter adds a line. Every key it acts on is claimed, so
     * Esc never reaches the window's `tags.close`.
     *
     * A text the row can't send (`blockedHint` answers why, e.g. the «multiple values» marker is still in it) keeps
     * the box open with that hint, on Enter and on a click away alike (the box takes focus back), so the typing is
     * never dropped without a word; Esc is the way to give it up. The hint follows the typing and goes once the text
     * can be sent. ⌘S (`tags.write`) commits like Enter and then reaches the window, which lists the changes. The pane
     * places the box (`place`) and moves it when the pane resizes; it scrolls itself into view on open and whenever
     * the hint appears, so neither a tall multiline box nor its hint near the pane's foot is cut off.
     */
    import { onMount, tick, untrack } from 'svelte'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import type { EditorPlace } from './pane-cursor'

    interface Props {
        id?: string
        /** The text the row holds; the box starts with it. */
        value: string
        ariaLabel: string
        multiline?: boolean
        invalid?: boolean
        describedBy?: string
        /** Where the box sits over its row, in the scroller's coordinates. */
        place?: EditorPlace
        /** Why `text` can't be sent as it is, or `null` when it can. Enter then keeps the box open with the words. */
        blockedHint?: (text: string) => string | null
        /** Called with the typed text, only when it differs from `value`. */
        onCommit: (text: string) => void | Promise<void>
        /**
         * The box is done (committed or cancelled); the pane drops it. `refocus`: focus went nowhere else (a key closed
         * it, or a click on no control), so the pane takes it back.
         */
        onClose: (refocus: boolean) => void
    }

    const {
        id,
        value,
        ariaLabel,
        multiline = false,
        invalid = false,
        describedBy,
        place,
        blockedHint,
        onCommit,
        onClose,
    }: Props = $props()

    const log = getAppLogger('tagEditor')
    const uid = $props.id()
    const hintId = `${uid}-hint`

    let draft = $state(untrack(() => value))
    let box = $state<HTMLInputElement | HTMLTextAreaElement>()
    let frame = $state<HTMLElement>()
    let hint = $state<string | null>(null)
    let done = false

    const describedByAll = $derived([describedBy, hint === null ? undefined : hintId].filter(Boolean).join(' ') || undefined)

    /** Brings the whole frame (box and hint) into the scroller's view. jsdom has no `scrollIntoView`. */
    function reveal(): void {
        if (frame && typeof frame.scrollIntoView === 'function') frame.scrollIntoView({ block: 'nearest' })
    }

    onMount(() => {
        box?.focus()
        box?.select()
        reveal()
    })

    // The hint grows the frame: show it, not only the box.
    $effect(() => {
        if (hint === null) return
        void tick().then(reveal)
    })

    /** Why the text typed so far can't be sent, or `null`; an untouched value is never refused (it sends nothing). */
    function refusal(text: string): string | null {
        return text === value ? null : (blockedHint?.(text) ?? null)
    }

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

    /** Enter: commits, unless the text can't be sent as it is; then the box stays and says why. */
    function tryCommit(): boolean {
        const why = refusal(draft)
        if (why === null) finish(true)
        else hint = why
        return why === null
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
            tryCommit()
        } else if (e.key === 'Escape') {
            claimKey(e)
            finish(false)
        } else if (eventMatchesCommand(e, 'tags.write')) {
            // Committed, the key goes on to the window, which lists the changes; refused, it stops here with the hint.
            if (!tryCommit()) claimKey(e)
        }
    }

    /** Typing: a shown hint follows the text, and goes once the text can be sent. */
    function oninput(e: Event): void {
        if (hint === null) return
        const target = e.currentTarget
        if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) hint = refusal(target.value)
    }

    /** A click away commits; a text that can't be sent keeps the box (focus comes back to it) with the hint. */
    function onblur(e: FocusEvent): void {
        if (done) return
        const why = refusal(draft)
        if (why === null) {
            finish(true, e.relatedTarget === null)
            return
        }
        hint = why
        setTimeout(() => {
            if (!done && box?.isConnected === true) box.focus()
        }, 0)
    }
</script>

<div
    bind:this={frame}
    class="tag-value-frame"
    style:top={place ? `${String(place.top)}px` : undefined}
    style:left={place ? `${String(place.left)}px` : undefined}
    style:width={place ? `${String(place.width)}px` : undefined}
>
    {#if multiline}
        <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- laid over a pane row like the inline rename field: it takes the row's font and box, which the framed `TextArea` (its padding, radius, ring) can't. -->
        <textarea
            bind:this={box}
            bind:value={draft}
            {id}
            class="tag-value-editor multiline"
            aria-label={ariaLabel}
            aria-invalid={invalid ? 'true' : undefined}
            aria-describedby={describedByAll}
            rows={4}
            {onkeydown}
            {oninput}
            {onblur}
        ></textarea>
    {:else}
        <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- laid over a pane row like the inline rename field (`InlineRenameEditor`): it takes the row's height and font, which the framed `TextInput`'s padding would break. -->
        <input
            bind:this={box}
            bind:value={draft}
            {id}
            type="text"
            class="tag-value-editor"
            style:height={place?.height === null || place === undefined ? undefined : `${String(place.height)}px`}
            aria-label={ariaLabel}
            aria-invalid={invalid ? 'true' : undefined}
            aria-describedby={describedByAll}
            {onkeydown}
            {oninput}
            {onblur}
        />
    {/if}
    {#if hint !== null}
        <p class="tag-value-hint" id={hintId} role="status">{hint}</p>
    {/if}
</div>

<style>
    .tag-value-frame {
        position: absolute;
        z-index: 1;
        /* Room under the box when it's the last thing in the scroller, so `scrollIntoView` shows all of it. */
        padding-bottom: var(--spacing-xs);
    }

    /* The inline rename field's look (`InlineRenameEditor`), in the accent rather than a severity color. */
    .tag-value-editor {
        display: block;
        box-sizing: border-box;
        width: 100%;
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

    .tag-value-hint {
        margin: var(--spacing-xxs) 0 0;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-normal);
        white-space: normal;
    }
</style>
