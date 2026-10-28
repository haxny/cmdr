<script lang="ts">
    /**
     * The Multi-Rename sheet's "Tags…" button (only mounted with the `multiRename.tagEditor` setting on): asks the
     * main window to open a Tags window over a copy of the sheet's session (`open-tags.ts`). The sheet stays open, and
     * closing it never ends the window's session.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import Button from '#lib/ui/Button.svelte'
    import { openTagsForSession } from './open-tags'

    interface Props {
        sessionId: string
    }

    const { sessionId }: Props = $props()
</script>

<div class="tags-button">
    <Button size="mini" onclick={() => void openTagsForSession(sessionId)}>{tString('multiRename.tags.openWindow')}</Button>
</div>

<style>
    .tags-button {
        display: flex;
        justify-content: flex-end;
        flex-shrink: 0;
    }
</style>
