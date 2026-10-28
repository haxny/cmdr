<script lang="ts">
    /**
     * The cover section, the Album pane's last group: every distinct cover the files carry as a thumbnail, with how
     * many files carry it and its size, and the files without one counted too (`list_tag_covers`, listed again on
     * every new view). A click opens a cover large (`onView`, the `CoverViewer` dialog), where "Use for all files"
     * gives it to every file. A picture pasted (⌘V) while the section has focus, or a picture file dropped on it,
     * becomes every file's cover, as does Paste picture (the mouse road, `navigator.clipboard.read()`); "From folder…" lists the pictures beside the files; "Remove" takes every cover out.
     * Each is an in-memory change Write lists; nothing touches a file before Write.
     *
     * Thumbnails are small PNG `data:` URLs the backend makes from each cover (`get_cover_thumbnail`; the CSP allows
     * `data:` images, not `blob:`), fetched once per cover key; the viewer fetches the full picture. ⌘V is the native Edit > Paste, which WebKit offers on a non-text focus
     * only when the page takes `beforepaste`; the `paste` then comes to the document, so both are listened to there,
     * and acted on only while focus is in this section. A drop is the webview's native drag-drop (`cover-drop.ts`).
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { CoverChoice, CoverSummary } from '#lib/ipc/bindings.js'
    import { getFileSizeFormat } from '#lib/settings/reactive-settings.svelte.js'
    import type { CoverCandidate, TagEdit } from '#lib/tauri-commands/index.js'
    import { formatFileSizeWithFormat } from '#lib/units/byte-size.js'
    import Button from '#lib/ui/Button.svelte'
    import { dependOn } from '#lib/utils/reactivity.js'
    import { NO_COVER_KEY, coverDetails, filesText, pasteLimit, pastedPicture, visiblePart, type CoverShown } from './cover-choices'
    import { isOver, subscribeCoverDrop, type CoverDropEvent, type CoverDropSubscriber } from './cover-drop'

    interface Props {
        cover: CoverSummary
        editable: boolean
        /** The view: a new one lists the covers again. */
        revision: unknown
        /** Every distinct cover with its count, or `null` when listing them failed. */
        listChoices: () => Promise<CoverChoice[] | null>
        /** A tile's small picture of one cover (`get_cover_thumbnail`); `null` when it can't be had. */
        loadThumbnail: (key: string) => Promise<string | null>
        /** The folder's pictures, or `null` when listing them failed. */
        listCandidates: () => Promise<CoverCandidate[] | null>
        onView: (shown: CoverShown) => void
        onPick: (fileName: string) => void
        onEdit: (edits: TagEdit[]) => void
        /** A pasted picture's bytes, at most 10 MiB. */
        onPaste: (data: Uint8Array) => void
        /** A dropped file's path. */
        onDrop: (path: string) => void
        /** The window's file drags (the webview's by default; tests give their own). */
        subscribeDrop?: CoverDropSubscriber
    }

    const {
        cover,
        editable,
        revision,
        listChoices,
        loadThumbnail,
        listCandidates,
        onView,
        onPick,
        onEdit,
        onPaste,
        onDrop,
        subscribeDrop = subscribeCoverDrop,
    }: Props = $props()

    const sizeText = (bytes: number): string => formatFileSizeWithFormat(bytes, getFileSizeFormat())

    let section = $state<HTMLElement>()

    /** `undefined` while the first listing is out, `null` when it failed. */
    let choices = $state<CoverChoice[] | null | undefined>(undefined)
    const total = $derived((choices ?? []).reduce((sum, choice) => sum + choice.files, 0))
    /** Each cover's `data:` URL by key, once it came. */
    let images = $state<Record<string, string | null>>({})
    /** The keys asked for already, so a cover is fetched once however often the list comes again. */
    let requested: Record<string, true> = {}
    /** Which listing the section waits for: an answer to an older view is dropped. */
    let choicesAsk = 0

    $effect(() => {
        dependOn(revision)
        // A new view answers the last paste or drop (or an edit made otherwise): its notice goes.
        notice = null
        const ask = ++choicesAsk
        void listChoices().then((listed) => {
            if (ask !== choicesAsk) return
            choices = listed
            if (listed !== null) loadImages(listed)
        })
    })

    function loadImages(listed: CoverChoice[]): void {
        const keys = listed.map((choice) => choice.key).filter((key) => key !== NO_COVER_KEY)
        images = Object.fromEntries(Object.entries(images).filter(([key]) => keys.includes(key)))
        requested = Object.fromEntries(keys.filter((key) => key in requested).map((key) => [key, true as const]))
        for (const key of keys.filter((key) => !(key in requested))) {
            requested[key] = true
            void fetchThumbnail(key).then((url) => {
                if (key in requested) images = { ...images, [key]: url }
            })
        }
    }

    /** A tile's picture, asked again once when the first answer is none (a timeout under load); `null` after both. */
    async function fetchThumbnail(key: string): Promise<string | null> {
        return (await loadThumbnail(key)) ?? (await loadThumbnail(key))
    }

    /** A paste or drop's answer here: no picture, too large, several files. The next one clears it. */
    let notice = $state<MessageKey | null>(null)

    function focusInside(): boolean {
        return section !== undefined && section.contains(document.activeElement)
    }

    /** A pasted picture, by ⌘V or the Paste picture button: refused here when too large, else sent. */
    async function takePicture(picture: Blob): Promise<void> {
        if (picture.size > pasteLimit(picture.type)) {
            notice = 'multiRename.tags.cover.tooLarge'
            return
        }
        notice = null
        onPaste(new Uint8Array(await picture.arrayBuffer()))
    }

    async function takePaste(e: ClipboardEvent): Promise<void> {
        e.preventDefault()
        const picture = pastedPicture(e.clipboardData)
        if (picture === null) {
            notice = 'multiRename.tags.cover.pasteNoImage'
            return
        }
        await takePicture(picture)
    }

    /** The Paste picture button: the mouse road to the clipboard (WebKit shows its own Paste callout first). */
    async function pasteFromClipboard(): Promise<void> {
        let items: ClipboardItem[]
        try {
            items = await navigator.clipboard.read()
        } catch {
            notice = 'multiRename.tags.cover.pasteUnavailable'
            return
        }
        for (const item of items) {
            const type = item.types.find((candidate) => candidate.startsWith('image/'))
            if (type !== undefined) {
                await takePicture(await item.getType(type))
                return
            }
        }
        notice = 'multiRename.tags.cover.pasteNoImage'
    }

    $effect(() => {
        if (!editable) return
        const onBeforePaste = (e: Event): void => {
            // WebKit enables Edit > Paste on a focus that isn't a text box only when the page takes this.
            if (focusInside()) e.preventDefault()
        }
        const onPasteEvent = (e: ClipboardEvent): void => {
            if (focusInside()) void takePaste(e)
        }
        document.addEventListener('beforepaste', onBeforePaste)
        document.addEventListener('paste', onPasteEvent)
        return () => {
            document.removeEventListener('beforepaste', onBeforePaste)
            document.removeEventListener('paste', onPasteEvent)
        }
    })

    /** A file is dragged over the section. */
    let dragOver = $state(false)

    /**
     * The part of the section a drop can land on: its rect cut to the scroller it sits in (`data-cover-clip`), so a
     * section scrolled out of view never takes a drop on what shows there (the rows' strip, the status line, the bar).
     */
    function dropRect(): DOMRect | null {
        if (!section) return null
        const clip = section.closest('[data-cover-clip]')?.getBoundingClientRect() ?? null
        return visiblePart(section.getBoundingClientRect(), clip)
    }

    function onDragEvent(event: CoverDropEvent): void {
        const rect = dropRect()
        if (!editable || event.type === 'leave') {
            dragOver = false
            return
        }
        const inside = isOver(event.position, rect)
        if (event.type !== 'drop') {
            dragOver = inside
            return
        }
        dragOver = false
        if (!inside) return
        if (event.paths.length !== 1) {
            notice = 'multiRename.tags.cover.dropOne'
            return
        }
        notice = null
        onDrop(event.paths[0])
    }

    $effect(() => {
        let stop: (() => void) | undefined
        let gone = false
        void subscribeDrop(onDragEvent).then((unsubscribe) => {
            if (gone) unsubscribe()
            else stop = unsubscribe
        })
        return () => {
            gone = true
            stop?.()
        }
    })

    function view(choice: CoverChoice): void {
        onView({ choice, total })
    }

    type Picker = { status: 'loading' } | { status: 'failed' } | { status: 'list'; candidates: CoverCandidate[] }
    let picker = $state<Picker | null>(null)
    /** Which listing the picker waits for: an answer to an earlier one (the button pressed again) is dropped. */
    let pickerAsk = 0

    async function openPicker(): Promise<void> {
        const ask = ++pickerAsk
        if (picker !== null) {
            picker = null
            return
        }
        picker = { status: 'loading' }
        const candidates = await listCandidates()
        if (ask !== pickerAsk) return
        picker = candidates === null ? { status: 'failed' } : { status: 'list', candidates }
    }

    function pick(fileName: string): void {
        picker = null
        onPick(fileName)
    }

    /** Focus comes to the section on a click anywhere in it, so ⌘V lands here (WebKit doesn't focus a clicked button). */
    function onmousedown(e: MouseEvent): void {
        if (e.target instanceof HTMLElement && e.target.closest('button') === null) section?.focus()
    }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- Focusable so ⌘V pastes a picture here; a click takes focus too (`onmousedown`). -->
<section
    bind:this={section}
    class="cover"
    class:drop-target={dragOver}
    aria-labelledby="tag-cover-heading"
    tabindex="0"
    {onmousedown}
>
    <h3 id="tag-cover-heading">{tString('multiRename.tags.cover.label')}</h3>
    {#if choices === null}
        <p role="alert">{tString('multiRename.tags.cover.choicesFailed')}</p>
    {:else if choices !== undefined && choices.length > 0}
        <ul class="choices" aria-label={tString('multiRename.tags.cover.choicesLabel')}>
            {#each choices as choice (choice.key)}
                <li>
                    {#if choice.key === NO_COVER_KEY}
                        <div class="choice">
                            <span class="frame empty">{tString('multiRename.tags.cover.none')}</span>
                            <span class="caption">{filesText(choice.files)}</span>
                        </div>
                    {:else}
                        {@const image = images[choice.key]}
                        {@const caption = tString('multiRename.tags.cover.caption', {
                            filesText: filesText(choice.files),
                            sizeText: sizeText(choice.bytes),
                        })}
                        <button
                            type="button"
                            class="choice"
                            aria-label={tString('multiRename.tags.cover.openLabel', {
                                caption,
                                details: coverDetails(choice, sizeText(choice.bytes)),
                            })}
                            onclick={() => { view(choice) }}
                        >
                            <span class="frame" class:empty={image === null}>
                                {#if image}
                                    <img src={image} alt="" />
                                {:else if image === null}
                                    {tString('multiRename.tags.cover.noPreview')}
                                {/if}
                            </span>
                            <span class="caption">{caption}</span>
                        </button>
                    {/if}
                </li>
            {/each}
        </ul>
    {/if}
    {#if dragOver}
        <p class="hint">{tString('multiRename.tags.cover.dropHere')}</p>
    {:else if editable}
        <p class="hint">{tString('multiRename.tags.cover.pasteHint')}</p>
    {/if}
    {#if notice !== null}
        <p class="notice" role="status">{tString(notice)}</p>
    {/if}
    <div class="actions">
        <Button disabled={!editable} onclick={() => { void pasteFromClipboard() }}>
            {tString('multiRename.tags.cover.pasteButton')}
        </Button>
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
        border-radius: var(--radius-sm);
        outline: none;
    }

    .cover:focus-visible,
    .cover.drop-target {
        outline: 2px dashed var(--color-accent);
        outline-offset: 2px;
    }

    h3 {
        margin: 0;
        font-size: var(--font-size-md);
    }

    /* Tiles wrap: the pane never scrolls sideways. */
    .choices {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-sm);
        margin: 0;
        padding: 0;
        list-style: none;
    }

    .choice {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        width: calc(88px * var(--font-scale));
        padding: var(--spacing-xxs);
        border: 1px solid transparent;
        border-radius: var(--radius-sm);
        background: none;
        color: var(--color-text-primary);
        font: inherit;
        text-align: center;
        cursor: default;
    }

    button.choice:hover {
        background: var(--color-accent-subtle);
    }

    button.choice:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .frame {
        display: flex;
        align-items: center;
        justify-content: center;
        aspect-ratio: 1;
        overflow: hidden;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
    }

    .frame img {
        width: 100%;
        height: 100%;
        object-fit: contain;
    }

    .frame.empty {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .caption,
    .hint {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        overflow-wrap: anywhere;
    }

    .hint,
    .notice {
        margin: 0;
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
