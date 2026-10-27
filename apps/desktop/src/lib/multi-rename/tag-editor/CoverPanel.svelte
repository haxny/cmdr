<script lang="ts">
    /**
     * The cover of the selected files: what they share (type, size, pixel size), «multiple covers», or none. "From
     * folder…" lists the pictures beside the files and gives the chosen one to every file; "Remove" takes the cover
     * out of every file. The view carries no picture bytes, so there is no thumbnail yet.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { CoverSummary } from '#lib/ipc/bindings.js'
    import { getFileSizeFormat } from '#lib/settings/reactive-settings.svelte.js'
    import type { CoverCandidate, TagEdit } from '#lib/tauri-commands/index.js'
    import { formatFileSizeWithFormat } from '#lib/units/byte-size.js'
    import Button from '#lib/ui/Button.svelte'

    interface Props {
        cover: CoverSummary
        editable: boolean
        /** The folder's pictures, or `null` when listing them failed. */
        listCandidates: () => Promise<CoverCandidate[] | null>
        onPick: (fileName: string) => void
        onEdit: (edits: TagEdit[]) => void
    }

    const { cover, editable, listCandidates, onPick, onEdit }: Props = $props()

    type Picker = { status: 'loading' } | { status: 'failed' } | { status: 'list'; candidates: CoverCandidate[] }
    let picker = $state<Picker | null>(null)

    const sizeText = (bytes: number): string => formatFileSizeWithFormat(bytes, getFileSizeFormat())

    const summary = $derived.by(() => {
        if (cover.kind === 'empty') return tString('multiRename.tags.cover.none')
        if (cover.kind === 'multiple') return tString('multiRename.tags.cover.multiple')
        if (cover.width !== null && cover.height !== null) {
            return tString('multiRename.tags.cover.detailsPixels', {
                mime: cover.mime,
                width: cover.width,
                height: cover.height,
                sizeText: sizeText(cover.bytes),
            })
        }
        return tString('multiRename.tags.cover.details', { mime: cover.mime, sizeText: sizeText(cover.bytes) })
    })

    async function openPicker(): Promise<void> {
        if (picker !== null) {
            picker = null
            return
        }
        picker = { status: 'loading' }
        const candidates = await listCandidates()
        picker = candidates === null ? { status: 'failed' } : { status: 'list', candidates }
    }

    function pick(fileName: string): void {
        picker = null
        onPick(fileName)
    }
</script>

<section class="cover" aria-labelledby="tag-cover-heading">
    <h3 id="tag-cover-heading">{tString('multiRename.tags.cover.label')}</h3>
    <p class="summary">{summary}</p>
    <div class="actions">
        <Button disabled={!editable} onclick={() => { void openPicker() }}>
            {tString('multiRename.tags.cover.fromFolder')}
        </Button>
        <Button disabled={!editable || cover.kind === 'empty'} onclick={() => { onEdit([{ kind: 'cover', edit: 'remove' }]) }}>
            {tString('multiRename.tags.cover.remove')}
        </Button>
    </div>
    {#if picker?.status === 'failed'}
        <p role="alert">{tString('multiRename.tags.cover.listFailed')}</p>
    {:else if picker?.status === 'list'}
        {#if picker.candidates.length === 0}
            <p>{tString('multiRename.tags.cover.noImages')}</p>
        {:else}
            <ul class="candidates" aria-label={tString('multiRename.tags.cover.pickerLabel')}>
                {#each picker.candidates as candidate (candidate.fileName)}
                    <li>
                        <button type="button" class="candidate" onclick={() => { pick(candidate.fileName) }}>
                            {tString('multiRename.tags.cover.candidate', {
                                fileName: candidate.fileName,
                                sizeText: sizeText(candidate.size),
                            })}
                        </button>
                    </li>
                {/each}
            </ul>
        {/if}
    {/if}
</section>

<style>
    .cover {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        min-width: 0;
    }

    h3 {
        margin: 0;
        font-size: var(--font-size-md);
    }

    .summary {
        margin: 0;
        color: var(--color-text-secondary);
    }

    .actions {
        display: flex;
        gap: var(--spacing-xs);
        flex-wrap: wrap;
    }

    .candidates {
        margin: 0;
        padding: 0;
        list-style: none;
        max-height: 160px;
        overflow: auto;
    }

    .candidate {
        width: 100%;
        padding: var(--spacing-xxs) var(--spacing-sm);
        border: none;
        background: none;
        color: var(--color-text-primary);
        font: inherit;
        text-align: left;
        cursor: default;
        overflow-wrap: break-word;
    }

    .candidate:hover {
        background: var(--color-accent-subtle);
    }

    .candidate:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }
</style>
