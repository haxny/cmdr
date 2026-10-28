<script lang="ts">
    /**
     * The Tags window's body (route `/tags`): the tag content of the Multi-Rename session the window holds, and the
     * way out. Escape (`tags.close`) and the red button (`requestClose`) go through the discard guard; closing
     * releases the hold, ends the tag session and the Multi-Rename session, then asks the backend to close the window
     * (`closeSelfWindow`, ❌ never `getCurrentWindow().close()` from a key handler). The backend ends the session too
     * when the window is destroyed some other way (`tag_editor/window.rs`). ⌘A and ⌘⇧A reach it from the native menu
     * (`onTagsSelectAll`, `onTagsDeselectAll`): ⌘A selects the text of the box being typed in, else every track;
     * ⌘⇧A clears the track selection.
     */
    import { onMount } from 'svelte'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { closeSelfWindow } from '#lib/child-window-close.js'
    import { closeMultiRename, onTagsDeselectAll, onTagsSelectAll, releaseTagsSession } from '#lib/tauri-commands/index.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import TagsTab from './TagsTab.svelte'
    import { createDiscardGuard } from './discard-guard'
    import { createTagEditorState } from './tag-editor-state.svelte'

    interface Props {
        sessionId: string
    }

    const { sessionId }: Props = $props()

    const log = getAppLogger('tagEditor')

    // The window shows one session for its whole life; a new session is a new window.
    // svelte-ignore state_referenced_locally
    const tags = createTagEditorState(sessionId)
    const guard = createDiscardGuard(tags, () => void end())
    let ended = false
    let content = $state<{ selectAllTracks: () => void; deselectAllTracks: () => void }>()

    /** A box that holds text to select (a checkbox or a button doesn't; its `selectionStart` is `null`). */
    function textBoxOf(el: Element | null): HTMLInputElement | HTMLTextAreaElement | null {
        if (el instanceof HTMLTextAreaElement) return el
        return el instanceof HTMLInputElement && el.selectionStart !== null ? el : null
    }

    /** ⌘A: the text of the box being typed in, as in any text field, else every track, as in a file pane. */
    function selectAll(): void {
        const box = textBoxOf(document.activeElement)
        if (box) box.select()
        else content?.selectAllTracks()
    }

    /** Listens to a menu road for the window's life; a listener that arrives after the window went is dropped. */
    function listenWhileMounted(subscribe: () => Promise<() => void>, what: string): () => void {
        let stop: (() => void) | undefined
        let gone = false
        subscribe()
            .then((unlisten) => {
                if (gone) unlisten()
                else stop = unlisten
            })
            .catch((e: unknown) => {
                log.warn('{what} from the menu won’t reach the tracks: {reason}', { what, reason: e })
            })
        return () => {
            gone = true
            stop?.()
        }
    }

    onMount(() => {
        const stops = [
            listenWhileMounted(() => onTagsSelectAll(selectAll), 'Select all'),
            listenWhileMounted(() => onTagsDeselectAll(() => content?.deselectAllTracks()), 'Deselect all'),
        ]
        return () => {
            for (const stop of stops) stop()
        }
    })

    /** True once the window is on its way out: the red button's second close-request must go through. */
    export function isEnding(): boolean {
        return ended
    }

    /** The red button: asks first when edits are unwritten, as Escape does. */
    export function requestClose(): void {
        guard.close()
    }

    /**
     * Releases the hold, ends the tag session (through the state, so a late reply is dropped) and the Multi-Rename
     * session, then closes the window, whatever the calls answer: the backend ends the session on `Destroyed` anyway.
     */
    async function end(): Promise<void> {
        if (ended) return
        ended = true
        try {
            await releaseTagsSession(sessionId)
            await tags.close()
            await closeMultiRename(sessionId)
        } catch (e) {
            log.warn("couldn't end the Tags window's session: {reason}", { reason: e })
        } finally {
            await closeSelfWindow()
        }
    }

    function onKeydown(e: KeyboardEvent): void {
        // A box undoing its typing or a dialog closing claims its Escape first.
        if (e.defaultPrevented || e.isComposing || !eventMatchesCommand(e, 'tags.close')) return
        claimKey(e)
        guard.close()
    }
</script>

<svelte:window onkeydown={onKeydown} />

<TagsTab bind:this={content} state={tags} />
