<script lang="ts">
    /**
     * The Tags window's body (route `/tags`): the tag content of the Multi-Rename session the window holds, and the
     * way out. Escape (`tags.close`) and the red button (`requestClose`) go through the discard guard; closing
     * releases the hold, ends the tag session and the Multi-Rename session, then asks the backend to close the window
     * (`closeSelfWindow`, ❌ never `getCurrentWindow().close()` from a key handler). While a write or an undo is on
     * its way nothing closes the window: a close request (Escape, ⌘W, the red button) shows "Writing tags…" in the
     * Write dialog instead, and once the write is done its outcome shows and closing works again. The backend ends the
     * session too when the window is destroyed some other way (`tag_editor/window.rs`). ⌘A and ⌘⇧A reach it from the
     * native menu (`onTagsSelectAll`, `onTagsDeselectAll`): ⌘A selects the text of the box being typed in, else every
     * track; ⌘⇧A clears the track selection. Under a dialog neither touches the tracks: the dialog's tool would run on
     * a selection it didn't show. ⌘X, ⌘C and ⌘V arrive the same way
     * (`onTagsClipboard`) and go to the content's `clipboard`: the field under the cursor, or the native action back
     * for a text box. The rest of the `Tags window` keys (`tags-function-keys.ts`) go to the content's
     * `runCommand`, except while a value is typed (only ⌘S, which the box has already committed, and the close keys get
     * through; the value editor claims its own Esc), while a dialog is open (it owns its keys, Escape included), and
     * Enter on a focused control (a button presses). On macOS the main menu this window installs holds F2–F8, ⇧F2,
     * ⇧F4 and ⇧F8 as accelerators that fire before the page: those presses arrive from the menu
     * (`onTagsMenuKey`), under the same rules, and a press seen by both roads acts once (`TagsContent`'s key echo).
     */
    import { onMount } from 'svelte'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { closeSelfWindow } from '#lib/child-window-close.js'
    import {
        closeMultiRename,
        onTagsClipboard,
        onTagsDeselectAll,
        onTagsMenuKey,
        onTagsSelectAll,
        releaseTagsSession,
        type TagsClipboardAction,
        type TagsMenuKey,
    } from '#lib/tauri-commands/index.js'
    import { getEffectiveShortcuts } from '#lib/shortcuts/shortcuts-store.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import TagsContent from './TagsContent.svelte'
    import { createDiscardGuard } from './discard-guard'
    import { createTagEditorState } from './tag-editor-state.svelte'
    import {
        PANE_KEYS,
        TAGS_WINDOW_KEYS,
        menuKeyCommand,
        textBoxOf,
        type TagsCommandId,
        type TagsKeyRoad,
    } from './tags-function-keys'

    interface Props {
        sessionId: string
    }

    const { sessionId }: Props = $props()

    const log = getAppLogger('tagEditor')

    // The window shows one session for its whole life; a new session is a new window.
    // svelte-ignore state_referenced_locally
    const tags = createTagEditorState(sessionId)
    const guard = createDiscardGuard(
        tags,
        () => void end(),
        () => content?.waitForWrite(),
    )
    let ended = false
    let content = $state<{
        selectAllTracks: () => void
        deselectAllTracks: () => void
        runCommand: (id: TagsCommandId, road?: TagsKeyRoad) => boolean
        hasDialog: () => boolean
        waitForWrite: () => void
        clipboard: (action: TagsClipboardAction) => Promise<void>
    }>()

    /** ⌘A: the text of the box being typed in, as in any text field, else every track, as in a file pane. */
    function selectAll(): void {
        const box = textBoxOf(document.activeElement)
        if (box) box.select()
        else if (content?.hasDialog() !== true) content?.selectAllTracks()
    }

    /** ⌘⇧A: no track selected, unless a dialog is open over them. */
    function deselectAll(): void {
        if (content?.hasDialog() !== true) content?.deselectAllTracks()
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
            listenWhileMounted(() => onTagsDeselectAll(deselectAll), 'Deselect all'),
            listenWhileMounted(() => onTagsMenuKey(onMenuKey), 'A function key'),
            listenWhileMounted(() => onTagsClipboard(onClipboard), 'Cut, copy or paste'),
        ]
        return () => {
            for (const stop of stops) stop()
        }
    })

    /** ⌘X / ⌘C / ⌘V from the menu: the content's; a call that doesn't go through is logged by kind, never retried. */
    function onClipboard(action: TagsClipboardAction): void {
        content?.clipboard(action).catch((e: unknown) => {
            log.warn('a cut, copy or paste from the menu didn’t go through: {reason}', { reason: e instanceof Error ? e.name : typeof e })
        })
    }

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

    /** The command a key means here, or `null`: what the key is, then whether it may act where it was pressed. */
    function commandOf(e: KeyboardEvent): TagsCommandId | 'tags.close' | null {
        const id = eventMatchesCommand(e, 'tags.close') ? 'tags.close' : TAGS_WINDOW_KEYS.find((key) => eventMatchesCommand(e, key))
        if (id === undefined) return null
        const target = e.target instanceof Element ? e.target : null
        // Typing a value: its own keys (⌘Z is its text undo); ⌘S gets here only once the box committed.
        if (textBoxOf(target) !== null) return id === 'tags.write' || id === 'tags.close' ? id : null
        // Enter belongs to the control that has focus (a button, a checkbox); a focused pane has already answered it.
        if (PANE_KEYS.has(id as TagsCommandId) && e.key === 'Enter' && target !== null && target !== document.body) return null
        return id
    }

    /**
     * A key the native menu delivered: the same rules as a keydown (nothing while a dialog is open or a value is
     * typed), and only while the menu item still holds the Tags command's key (`menuKeyCommand`).
     */
    function onMenuKey(key: TagsMenuKey): void {
        const id = menuKeyCommand(key, getEffectiveShortcuts)
        if (id === null || content?.hasDialog() === true || textBoxOf(document.activeElement) !== null) return
        content?.runCommand(id, 'menu')
    }

    function onKeydown(e: KeyboardEvent): void {
        // A box undoing its typing, a pane, or a dialog claims its keys first; a dialog owns them all.
        if (e.defaultPrevented || e.isComposing || content?.hasDialog() === true) return
        const id = commandOf(e)
        if (id === null) return
        claimKey(e)
        if (id === 'tags.close') {
            guard.close()
            return
        }
        // ⌘S from a text box other than the value editor (which committed already): leave it first, so its change lands.
        const box = textBoxOf(e.target instanceof Element ? e.target : null)
        if (box?.isConnected === true) box.blur()
        content?.runCommand(id, 'keyboard')
    }
</script>

<svelte:window onkeydown={onKeydown} />

<TagsContent bind:this={content} state={tags} />
