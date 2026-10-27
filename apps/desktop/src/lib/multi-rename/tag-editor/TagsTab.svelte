<script lang="ts">
    /**
     * The Tags tab of the Multi-Rename sheet, foobar2000 Properties style: the tags the files share, the track list
     * (title and number edit in place) and the cover. It opens the tag session the first time it shows.
     */
    import { onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import CoverPanel from './CoverPanel.svelte'
    import SharedFieldsTable from './SharedFieldsTable.svelte'
    import TrackList from './TrackList.svelte'
    import type { TagEditorState, TagStateError } from './tag-editor-state.svelte'

    interface Props {
        state: TagEditorState
    }

    const { state: tags }: Props = $props()

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
            default:
                return 'multiRename.tags.editFailed'
        }
    }

    async function listCandidates() {
        const res = await tags.coverCandidates()
        return res.ok ? res.value : null
    }
</script>

<div class="tags-tab" id="sheet-panel-tags" role="tabpanel" aria-labelledby="sheet-tab-tags">
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
        <div class="columns">
            <div class="main">
                <SharedFieldsTable rows={view.shared} editable={view.editable} onEdit={(edits) => void tags.edit(edits)} />
                <TrackList tracks={view.tracks} editable={view.editable} onEdit={(edits) => void tags.edit(edits)} />
            </div>
            <aside class="side">
                <CoverPanel
                    cover={view.cover}
                    editable={view.editable}
                    {listCandidates}
                    onPick={(fileName) => void tags.coverFromFile(fileName)}
                    onEdit={(edits) => void tags.edit(edits)}
                />
            </aside>
        </div>
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
