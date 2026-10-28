<script lang="ts">
    /**
     * One cover of the files, large, in a dialog over the panes: the picture, what it is (type, pixel size, size), on
     * how many of the files it is, and "Use for all files", which gives it to every file in memory (a change Write
     * lists). The button is off when every file has it already, or read-only. Esc or Close closes it.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { CoverChoice } from '#lib/ipc/bindings.js'
    import { getFileSizeFormat } from '#lib/settings/reactive-settings.svelte.js'
    import { formatFileSizeWithFormat } from '#lib/units/byte-size.js'
    import Button from '#lib/ui/Button.svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import { coverDetails } from './cover-choices'

    interface Props {
        choice: CoverChoice
        /** How many readable files there are. */
        total: number
        /** The cover as a `data:` URL, or `null` when it can't be shown. */
        image: string | null
        editable: boolean
        onUseForAll: (key: string) => void
        onClose: () => void
    }

    const { choice, total, image, editable, onUseForAll, onClose }: Props = $props()

    const details = $derived(coverDetails(choice, formatFileSizeWithFormat(choice.bytes, getFileSizeFormat())))
</script>

<ModalDialog titleId="tag-cover-viewer-title" containerStyle="width: 640px; max-width: 90vw" onclose={onClose}>
    {#snippet title()}{tString('multiRename.tags.cover.viewerTitle')}{/snippet}
    <div class="viewer">
        {#if image !== null}
            <img src={image} alt={tString('multiRename.tags.cover.viewerAlt', { details })} />
        {:else}
            <p class="cant-show" role="status">{tString('multiRename.tags.cover.cantShow')}</p>
        {/if}
        <p class="details">{details}</p>
        <p class="details">
            {tString('multiRename.tags.cover.onFiles', {
                countText: formatInteger(choice.files),
                totalText: formatInteger(total),
            })}
        </p>
    </div>
    {#snippet footer()}
        <Button onclick={onClose}>{tString('multiRename.tags.dialog.close')}</Button>
        <Button variant="primary" disabled={!editable || choice.files >= total} onclick={() => { onUseForAll(choice.key) }}>
            {tString('multiRename.tags.cover.useForAll')}
        </Button>
    {/snippet}
</ModalDialog>

<style>
    .viewer {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    img {
        max-width: 100%;
        max-height: 60vh;
        object-fit: contain;
        border-radius: var(--radius-sm);
    }

    .details,
    .cant-show {
        margin: 0;
        color: var(--color-text-secondary);
    }
</style>
