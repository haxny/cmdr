<script lang="ts">
    /**
     * One cover of the files, large, in a dialog over the panes: the full picture (fetched on open; the tiles show small
     * thumbnails), what it is (type, pixel size, size), on
     * how many of the files it is, and "Use for all files", which gives it to every file in memory (a change Write
     * lists). The button is off when every file has it already, or read-only. Esc or Close closes it.
     */
    import { onMount } from 'svelte'
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
        /** The full cover as a `data:` URL (`get_cover_image`); `null` when it can't be shown. */
        loadImage: () => Promise<string | null>
        editable: boolean
        onUseForAll: (key: string) => void
        onClose: () => void
    }

    const { choice, total, loadImage, editable, onUseForAll, onClose }: Props = $props()

    /** `undefined` while the picture is on its way. */
    let image = $state<string | null | undefined>(undefined)
    onMount(() => {
        let gone = false
        void loadImage().then((url) => {
            if (!gone) image = url
        })
        return () => {
            gone = true
        }
    })

    const details = $derived(coverDetails(choice, formatFileSizeWithFormat(choice.bytes, getFileSizeFormat())))
</script>

<ModalDialog titleId="tag-cover-viewer-title" containerStyle="width: 640px; max-width: 90vw" onclose={onClose}>
    {#snippet title()}{tString('multiRename.tags.cover.viewerTitle')}{/snippet}
    <div class="viewer">
        {#if image === undefined}
            <p class="cant-show" role="status">{tString('multiRename.tags.cover.loading')}</p>
        {:else if image !== null}
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
        <Button variant="primary" disabled={!editable || (choice.files >= total && choice.nearDuplicates === 0)} onclick={() => { onUseForAll(choice.key) }}>
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
