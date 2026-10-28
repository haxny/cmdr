<script lang="ts">
    /**
     * The Tags window's content: the clean-up tools, then two panes like the main window's file panes, Album (the
     * tags the files share, then the cover) | Tracks (selected like files; the tools work on the selection), then the
     * write (`ApplyPanel`). Tab switches the panes (`tags-panes.svelte.ts`). It opens the tag session when it mounts.
     * The Tags window (`TagsWindow.svelte`) shows it.
     */
    import { onDestroy, onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { TagField, TagTool } from '#lib/ipc/bindings.js'
    import { openTagsInPlayer, type TagEdit } from '#lib/tauri-commands/index.js'
    import Button from '#lib/ui/Button.svelte'
    import AlbumPane from './AlbumPane.svelte'
    import ApplyPanel from './ApplyPanel.svelte'
    import CoverPanel from './CoverPanel.svelte'
    import TagToolsBar from './TagToolsBar.svelte'
    import TracksPane from './TracksPane.svelte'
    import type { RunScoped } from './tag-tools'
    import { createTagsPanes, type TagsPaneId } from './tags-panes.svelte'
    import { createTrackSelection } from './track-selection.svelte'
    import { createTrackPlayer } from './track-player.svelte'
    import type { TagsTabApi } from './sheet-tab'
    import type { TagEditorState, TagStateError } from './tag-editor-state.svelte'

    interface Props {
        state: TagEditorState
    }

    const { state: tags }: Props = $props()

    /** The selected tracks; the tools work on these alone. Created once per mount, synced on every view change. */
    const selection = createTrackSelection()
    const allPaths = $derived(tags.view?.tracks.map((track) => track.path) ?? [])
    /** Changes with the selected tracks and the track list: a file-name preview is only good for the tracks it listed. */
    const scopeKey = $derived(JSON.stringify([[...selection.selected].sort(), allPaths]))

    let selectionGone = $state(false)

    // Converges: `sync` writes only when the list of paths changed.
    $effect(() => {
        selection.sync(allPaths)
    })

    // Selecting again answers the "no longer here" notice.
    $effect(() => {
        if (selection.count > 0) selectionGone = false
    })

    let albumPane = $state<{ focus: () => void }>()
    let tracksPane = $state<{ focus: () => void }>()
    const panes = createTagsPanes({
        focus: (pane) => {
            ;(pane === 'album' ? albumPane : tracksPane)?.focus()
        },
    })

    // The window opens with the Album pane focused, as the main window opens with a file pane focused.
    let focusedOnce = false
    $effect(() => {
        if (focusedOnce || albumPane === undefined) return
        focusedOnce = true
        albumPane.focus()
    })

    /** ⌘A from the native menu (`TagsWindow`): every track, as in a file pane. */
    export function selectAllTracks(): void {
        selection.selectAll()
    }

    /** ⌘⇧A from the native menu (`TagsWindow`): no track selected. */
    export function deselectAllTracks(): void {
        selection.deselectAll()
    }

    /** Focus is inside one of the panes (not the tools bar, the cover, or Write): only then a pane draws as focused. */
    let inPanes = $state(false)

    onMount(() => {
        void tags.open()
    })

    /**
     * The tracks play here, one at a time. Closing the window unmounts this component: playback stops and its URL is
     * released.
     */
    const player = createTrackPlayer(tags.sessionId)
    onDestroy(() => {
        player.stop()
    })

    /** Why the last Open in foobar2000 failed, when it did. */
    let playerError = $state<MessageKey | null>(null)

    /** Opens the selected tracks (every track when none is selected) in foobar2000, else each in its default app. */
    async function openInPlayer(): Promise<void> {
        playerError = null
        // Unreadable tracks are not the session's tracks for the backend (it refuses the call for one), so they never go.
        const readable = new Set(tags.view?.tracks.filter((track) => track.readError === null).map((track) => track.path))
        const res = await runScoped(async (scope) => {
            const paths = (scope ?? allPaths).filter((path) => readable.has(path))
            if (paths.length === 0) return 'nothingReadable' as const
            return await openTagsInPlayer(tags.sessionId, paths)
        })
        if (res === undefined) return
        if (res === 'nothingReadable') {
            playerError = 'multiRename.tags.play.nothingReadable'
            return
        }
        if (res.ok) return
        playerError =
            res.error.kind === 'playerUnavailable' ? 'multiRename.tags.play.playerUnavailable' : 'multiRename.tags.play.openFailed'
    }

    /** What a failed edit or cover pick says; a failure to read the tags at all is worded where there is no view. */
    function editErrorKey(error: TagStateError): MessageKey | null {
        switch (error.kind) {
            // The text tools' panels word these where the user typed them.
            case 'badRegex':
            case 'badMask':
            case 'badPattern':
                return null
            case 'coverTooLarge':
                return 'multiRename.tags.cover.tooLarge'
            case 'badCoverName':
            case 'coverUnreadable':
                return 'multiRename.tags.cover.unreadable'
            case 'coverNotAnImage':
                return 'multiRename.tags.cover.notAnImage'
            case 'undo':
            case 'noAppDataDir':
                return 'multiRename.tags.undoStoreUnavailable'
            default:
                return 'multiRename.tags.editFailed'
        }
    }

    let toolsBar = $state<HTMLElement>()

    /**
     * The field Case changes: the Album pane's cursor row while that pane is active, Title while the Tracks pane is,
     * none otherwise. Focus moving into the tools bar keeps the pane, so Case reaches the field the user just left
     * for the button; focus anywhere else (the cover, the Apply panel) or out of the window clears it.
     */
    const focusedField = $derived.by((): TagField | null => {
        if (panes.active === 'tracks') return 'title'
        if (panes.active !== 'album') return null
        const shared = tags.view?.shared ?? []
        return shared.at(Math.max(0, Math.min(panes.albumCursor, shared.length - 1)))?.field ?? null
    })

    function followFocus(target: EventTarget | null): void {
        if (target instanceof Node && toolsBar?.contains(target) === true) {
            // Case and Replace keep the pane's field; the pane no longer draws its cursor as focused.
            inPanes = false
            return
        }
        const slot = target instanceof Element ? target.closest<HTMLElement>('[data-pane]') : null
        const inCover = target instanceof Element && target.closest('.cover-group') !== null
        inPanes = slot !== null && !inCover
        panes.noteFocus(inPanes && slot !== null ? (slot.dataset.pane as TagsPaneId) : null)
    }
    let applyPanel = $state<TagsTabApi>()

    /**
     * A tool works on what the files hold now, so an edit a box just sent lands first, and on the selected tracks
     * alone (`null` for all). Resolves `undefined` without calling when the selected tracks are gone.
     */
    const runScoped: RunScoped = async (call) => {
        await tags.settled()
        const scope = selection.scope()
        if (scope.kind === 'gone') {
            // The selected tracks are gone: never widen to every track. The next run takes them all.
            selection.clearGone()
            selectionGone = true
            return undefined
        }
        selectionGone = false
        return await call(scope.kind === 'some' ? scope.paths : null)
    }

    async function runTool(tool: TagTool): Promise<void> {
        await runScoped((scope) => tags.tool(tool, scope))
    }

    /** Start: the Apply panel lists the changes, then writes them. */
    export async function start(): Promise<void> {
        await applyPanel?.start()
    }

    async function listCandidates() {
        const res = await tags.coverCandidates()
        return res.ok ? res.value : null
    }
</script>

<div
    class="tags-tab"
    id="sheet-panel-tags"
    onfocusin={(e) => { followFocus(e.target) }}
    onfocusout={(e) => { if (!(e.relatedTarget instanceof Node) || !e.currentTarget.contains(e.relatedTarget)) { inPanes = false; panes.noteFocus(null) } }}
>
    {#if tags.view === null}
        {#if tags.error !== null}
            <p role="alert">{tString('multiRename.tags.error')}</p>
        {:else}
            <p role="status">{tString('multiRename.tags.reading')}</p>
        {/if}
    {:else}
        {@const view = tags.view}
        {#if !view.editable}
            <p class="notice" role="status">{tString('multiRename.tags.readOnly')}</p>
        {/if}
        {#if tags.error !== null}
            {@const errorKey = editErrorKey(tags.error)}
            {#if errorKey !== null}<p class="notice" role="alert">{tString(errorKey)}</p>{/if}
        {/if}
        {#if player.failed}
            <p class="notice" role="alert">{tString('multiRename.tags.play.failed')}</p>
        {/if}
        {#if playerError !== null}
            <p class="notice" role="alert">{tString(playerError)}</p>
        {/if}
        {#if selectionGone}
            <p class="notice" role="status">{tString('multiRename.tags.selectionGone')}</p>
        {/if}
        <div bind:this={toolsBar}>
        <TagToolsBar
            trackCount={view.tracks.length}
            extras={view.extras}
            {focusedField}
            disabled={!view.editable}
            onTool={(tool: TagTool) => void runTool(tool)}
            state={tags}
            {runScoped}
            {scopeKey}
        />
        </div>
        <div class="panes">
            <div class="pane-slot album" data-pane="album">
                <AlbumPane
                    bind:this={albumPane}
                    rows={view.shared}
                    editable={view.editable}
                    badDates={view.badDates}
                    focused={inPanes && panes.active === 'album'}
                    cursor={panes.albumCursor}
                    onCursorChange={(index: number) => { panes.albumCursor = index }}
                    onEdit={(edits: TagEdit[]) => tags.edit(edits)}
                    onSwitchPane={() => { panes.switchPane() }}
                    cover={coverGroup}
                />
            </div>
            <div class="divider" aria-hidden="true"></div>
            <div class="pane-slot tracks" data-pane="tracks">
                <TracksPane
                    bind:this={tracksPane}
                    tracks={view.tracks}
                    editable={view.editable}
                    focused={inPanes && panes.active === 'tracks'}
                    cursor={panes.tracksCursor}
                    onCursorChange={(index: number) => { panes.tracksCursor = index }}
                    {selection}
                    playing={player.playing}
                    onPlay={(path: string) => void player.toggle(path)}
                    onEdit={(edits: TagEdit[]) => tags.edit(edits)}
                    onSwitchPane={() => { panes.switchPane() }}
                    footer={tracksFooter}
                />
            </div>
        </div>
        <ApplyPanel bind:this={applyPanel} {tags} onWriting={() => { player.stop() }} />
    {/if}
</div>

{#snippet coverGroup()}
    {#if tags.view !== null}
        <CoverPanel
            cover={tags.view.cover}
            editable={tags.view.editable}
            {listCandidates}
            loadImage={() => tags.coverImage()}
            onPick={(fileName: string) => void tags.coverFromFile(fileName)}
            onEdit={(edits: TagEdit[]) => tags.edit(edits)}
        />
    {/if}
{/snippet}

{#snippet tracksFooter()}
    <Button size="mini" onclick={() => void openInPlayer()}>{tString('multiRename.tags.play.openInPlayer')}</Button>
{/snippet}

<style>
    .tags-tab {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        flex: 1;
        min-height: 0;
        padding: var(--spacing-sm) 0;
        color: var(--color-text-primary);
    }

    /* Everything but the panes keeps the window's margin; the panes run edge to edge, as in the main window. */
    .tags-tab > :global(:not(.panes)) {
        margin-inline: var(--spacing-md);
    }

    /* Album | Tracks, split like the main window's panes: a 1px divider in `--color-border` (`PaneResizer`). */
    .panes {
        display: flex;
        flex: 1;
        min-height: 0;
        border-block: 1px solid var(--color-border);
    }

    .pane-slot {
        display: flex;
        flex-direction: column;
        min-width: 0;
        min-height: 0;
    }

    .pane-slot.album {
        flex: 45 1 0;
    }

    .pane-slot.tracks {
        flex: 55 1 0;
    }

    .divider {
        width: 1px;
        flex-shrink: 0;
        background: var(--color-border);
    }

    .notice {
        margin: 0;
        color: var(--color-text-secondary);
    }
</style>
