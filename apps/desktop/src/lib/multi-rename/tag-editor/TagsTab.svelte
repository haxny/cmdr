<script lang="ts">
    /**
     * The Tags tab of the Multi-Rename sheet, foobar2000 Properties style: the clean-up tools, the tags the files
     * share, the track list (title and number edit in place), the cover, and the write (`ApplyPanel`). It opens the tag
     * session the first time it shows. The sheet's Start comes to `start()` while the tab shows.
     */
    import { onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { TagField, TagTool } from '#lib/ipc/bindings.js'
    import type { TagEdit } from '#lib/tauri-commands/index.js'
    import ApplyPanel from './ApplyPanel.svelte'
    import CoverPanel from './CoverPanel.svelte'
    import SharedFieldsTable from './SharedFieldsTable.svelte'
    import TagToolsBar from './TagToolsBar.svelte'
    import TrackList from './TrackList.svelte'
    import { focusedFieldOf } from './tag-tools'
    import { createTrackSelection } from './track-selection.svelte'
    import type { TagsTabApi } from './sheet-tab'
    import type { TagEditorState, TagStateError } from './tag-editor-state.svelte'

    interface Props {
        state: TagEditorState
    }

    const { state: tags }: Props = $props()

    /** The checked tracks; the tools work on these alone. Created once per mount, pruned on every view change. */
    const selection = createTrackSelection()
    const allPaths = $derived(tags.view?.tracks.map((track) => track.path) ?? [])

    let selectionGone = $state(false)

    // Converges: `prune` writes only when a checked path is dropped.
    $effect(() => {
        selection.prune(allPaths)
    })

    onMount(() => {
        void tags.open()
    })

    /** What a failed edit or cover pick says; a failure to read the tags at all is worded where there is no view. */
    function editErrorKey(error: TagStateError): MessageKey {
        switch (error.kind) {
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

    /** The field whose box last had focus, for Case; focus on anything else in the tags clears it. */
    let focusedField = $state<TagField | null>(null)
    let toolsBar = $state<HTMLElement>()

    /**
     * Focus anywhere in the tab names the field Case changes: a tag box names its field, anything else (the cover,
     * the Apply panel, a Clear button) none. Focus moving into the tools bar keeps it, so Case reaches the field
     * the user just left for the button; focus leaving the tab clears it.
     */
    function followFocus(target: EventTarget | null): void {
        if (target instanceof Node && toolsBar?.contains(target) === true) return
        focusedField = target instanceof HTMLElement ? focusedFieldOf(target.id) : null
    }
    let applyPanel = $state<TagsTabApi>()

    /** A tool works on what the files hold now, so an edit a blur just sent lands first. */
    async function runTool(tool: TagTool): Promise<void> {
        await tags.settled()
        const scope = selection.scope(allPaths)
        if (scope.kind === 'gone') {
            // The checked tracks are gone: never widen to every track. The next run takes them all.
            selection.clearGone()
            selectionGone = true
            return
        }
        selectionGone = false
        await tags.tool(tool, scope.kind === 'some' ? scope.paths : null)
    }

    /** The sheet's Start while this tab shows: the Apply panel lists the changes, then writes them. */
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
    role="tabpanel"
    aria-labelledby="sheet-tab-tags"
    onfocusin={(e) => { followFocus(e.target) }}
    onfocusout={(e) => { if (e.relatedTarget instanceof Node && !e.currentTarget.contains(e.relatedTarget)) focusedField = null }}
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
            <p class="notice" role="alert">{tString(editErrorKey(tags.error))}</p>
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
        />
        </div>
        <div class="columns">
            <div class="main">
                <SharedFieldsTable rows={view.shared} editable={view.editable} onEdit={(edits: TagEdit[]) => tags.edit(edits)} />
                <TrackList
                    tracks={view.tracks}
                    editable={view.editable}
                    selected={selection.selected}
                    onToggle={(path: string) => { selectionGone = false; selection.toggle(path) }}
                    onToggleAll={(on: boolean) => { selectionGone = false; selection.setAll(allPaths, on) }}
                    onEdit={(edits: TagEdit[]) => tags.edit(edits)} />
            </div>
            <aside class="side">
                <CoverPanel
                    cover={view.cover}
                    editable={view.editable}
                    {listCandidates}
                    loadImage={() => tags.coverImage()}
                    onPick={(fileName: string) => void tags.coverFromFile(fileName)}
                    onEdit={(edits: TagEdit[]) => tags.edit(edits)}
                />
            </aside>
        </div>
        <ApplyPanel bind:this={applyPanel} {tags} />
    {/if}
</div>

<style>
    .tags-tab {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        flex: 1;
        min-height: 0;
        padding: var(--spacing-md);
        color: var(--color-text-primary);
    }

    .columns {
        display: flex;
        gap: var(--spacing-lg);
        flex: 1;
        min-height: 0;
    }

    .main {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
        flex: 1;
        min-width: 0;
        min-height: 0;
        overflow: auto;
    }

    .side {
        width: 240px;
        flex-shrink: 0;
    }

    .notice {
        margin: 0;
        color: var(--color-text-secondary);
    }
</style>
