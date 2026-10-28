<script lang="ts">
    /**
     * The choice a write asks for when some files would lose tag data the editor doesn't show (`TagError::WouldDrop`):
     * each file and its items, then "Write anyway" or "Skip these files". Never answered for the user. It sits in the
     * Tags window, under the write's report; focus starts on the safe choice.
     */
    import { onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { formatUnitList } from '#lib/intl/list-format.js'
    import type { WouldDropRow } from '#lib/ipc/bindings.js'
    import Button from '#lib/ui/Button.svelte'
    import { fileNameOf } from './apply-outcome'

    interface Props {
        rows: WouldDropRow[]
        /** The answer is being written: both buttons rest. */
        busy: boolean
        onWriteAnyway: () => void
        onSkip: () => void
    }

    const { rows, busy, onWriteAnyway, onSkip }: Props = $props()

    let section = $state<HTMLElement>()

    // "Skip these files" is the last button: the choice that loses nothing.
    onMount(() => {
        ;[...(section?.querySelectorAll('button') ?? [])].at(-1)?.focus()
    })
</script>

<section
    bind:this={section}
    class="would-drop"
    role="group"
    aria-labelledby="tag-would-drop-title"
    aria-describedby="tag-would-drop-message"
>
    <h3 id="tag-would-drop-title">{tString('multiRename.tags.wouldDrop.title')}</h3>
    <p id="tag-would-drop-message">
        {tString('multiRename.tags.wouldDrop.message', { count: rows.length, countText: formatInteger(rows.length) })}
    </p>
    <ul aria-label={tString('multiRename.tags.wouldDrop.listLabel')}>
        {#each rows as row (row.path)}
            <li>
                {tString('multiRename.tags.wouldDrop.row', {
                    fileName: fileNameOf(row.path),
                    items: formatUnitList(row.items),
                })}
            </li>
        {/each}
    </ul>
    <div class="actions">
        <Button disabled={busy} onclick={onWriteAnyway}>{tString('multiRename.tags.wouldDrop.writeAnyway')}</Button>
        <Button variant="primary" disabled={busy} onclick={onSkip}>{tString('multiRename.tags.wouldDrop.skip')}</Button>
    </div>
</section>

<style>
    .would-drop {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    h3,
    p {
        margin: 0;
    }

    h3 {
        font-size: var(--font-size-md);
    }

    ul {
        margin: 0;
        padding-left: var(--spacing-lg);
        max-height: 140px;
        overflow: auto;
        overflow-wrap: anywhere;
    }

    .actions {
        display: flex;
        gap: var(--spacing-xs);
    }
</style>
