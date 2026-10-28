<script lang="ts">
    /**
     * The Tags window's body (route `/tags`): the tag content of the Multi-Rename session the window holds, and the
     * way out. Escape (`tags.close`) and the red button (`requestClose`) go through the discard guard; closing
     * releases the hold, ends the tag session and the Multi-Rename session, then asks the backend to close the window
     * (`closeSelfWindow`, ❌ never `getCurrentWindow().close()` from a key handler). The backend ends the session too
     * when the window is destroyed some other way (`tag_editor/window.rs`).
     */
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { closeSelfWindow } from '#lib/child-window-close.js'
    import { closeMultiRename, closeTagEditor, releaseTagsSession } from '#lib/tauri-commands/index.js'
    import TagsTab from './TagsTab.svelte'
    import { createDiscardGuard } from './discard-guard'
    import { createTagEditorState } from './tag-editor-state.svelte'

    interface Props {
        sessionId: string
    }

    const { sessionId }: Props = $props()

    // The window shows one session for its whole life; a new session is a new window.
    // svelte-ignore state_referenced_locally
    const tags = createTagEditorState(sessionId)
    const guard = createDiscardGuard(tags, () => void end())
    let ended = false

    /** True once the window is on its way out: the red button's second close-request must go through. */
    export function isEnding(): boolean {
        return ended
    }

    /** The red button: asks first when edits are unwritten, as Escape does. */
    export function requestClose(): void {
        guard.close()
    }

    async function end(): Promise<void> {
        if (ended) return
        ended = true
        await releaseTagsSession(sessionId)
        await closeTagEditor(sessionId)
        await closeMultiRename(sessionId)
        await closeSelfWindow()
    }

    function onKeydown(e: KeyboardEvent): void {
        // A box undoing its typing or a dialog closing claims its Escape first.
        if (e.defaultPrevented || e.isComposing || !eventMatchesCommand(e, 'tags.close')) return
        claimKey(e)
        guard.close()
    }
</script>

<svelte:window onkeydown={onKeydown} />

<TagsTab state={tags} />
