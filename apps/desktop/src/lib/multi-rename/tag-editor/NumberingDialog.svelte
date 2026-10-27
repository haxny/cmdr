<script lang="ts">
    /**
     * The auto-numbering wizard, a copy of Mp3tag's: Track number (begin at, save total, leading zeros) and an
     * optional Disc number section (begin at, save total, leading zeros, next disc on change of directory or of a
     * field, reset the track counter on the next disc, reset the disc counter on change of a field). OK sends the
     * `autoNumber` tool, through `runScoped` so it numbers the checked tracks, then remembers the choices; Cancel
     * sends nothing. A single-file book gets no track number (the engine decides from the book's size; the dialog
     * says so when the book has one track).
     */
    import { onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { DiscOptions, NumberOptions, TagField } from '#lib/ipc/bindings.js'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Select from '#lib/ui/Select.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import { FIELD_LABELS, TAG_FIELDS } from './field-labels'
    import {
        DEFAULT_CHANGE_FIELD,
        DEFAULT_DISC,
        DEFAULT_NUMBERING,
        loadNumbering,
        parseBegin,
        saveNumbering,
    } from './numbering-options'
    import type { TagEditorState } from './tag-editor-state.svelte'
    import type { RunScoped } from './tag-tools'

    interface Props {
        /** The book's track count (not the checked ones). */
        trackCount: number
        state: TagEditorState
        runScoped: RunScoped
        onClose: () => void
    }

    const { trackCount, state: tags, runScoped, onClose }: Props = $props()

    const FIELD_ITEMS = $derived(TAG_FIELDS.map((field) => ({ value: field, label: tString(FIELD_LABELS[field]) })))

    let loaded = $state(false)
    let busy = $state(false)
    let trackStart = $state(String(DEFAULT_NUMBERING.trackStart))
    let trackTotal = $state(DEFAULT_NUMBERING.trackTotal)
    let trackPad = $state(DEFAULT_NUMBERING.trackPad)
    let discOn = $state(false)
    let discStart = $state(String(DEFAULT_DISC.start))
    let discTotal = $state(DEFAULT_DISC.total)
    let discPad = $state(DEFAULT_DISC.pad)
    let nextOnFolder = $state(DEFAULT_DISC.nextOnFolder)
    let nextOnFieldOn = $state(false)
    let nextOnField = $state<TagField>(DEFAULT_CHANGE_FIELD)
    let resetTrack = $state(DEFAULT_DISC.resetTrack)
    let resetDiscOn = $state(false)
    let resetDiscField = $state<TagField>(DEFAULT_CHANGE_FIELD)

    const trackBegin = $derived(parseBegin(trackStart))
    const discBegin = $derived(parseBegin(discStart))
    const trackInvalid = $derived(trackBegin === null)
    const discInvalid = $derived(discOn && discBegin === null)
    const canSubmit = $derived(loaded && !busy && !trackInvalid && !discInvalid)

    onMount(() => {
        void loadNumbering().then((saved) => {
            trackStart = String(saved.trackStart)
            trackTotal = saved.trackTotal
            trackPad = saved.trackPad
            const disc = saved.disc
            discOn = disc !== null
            if (disc !== null) {
                discStart = String(disc.start)
                discTotal = disc.total
                discPad = disc.pad
                nextOnFolder = disc.nextOnFolder
                nextOnFieldOn = disc.nextOnField !== null
                nextOnField = disc.nextOnField ?? DEFAULT_CHANGE_FIELD
                resetTrack = disc.resetTrack
                resetDiscOn = disc.resetDiscOnField !== null
                resetDiscField = disc.resetDiscOnField ?? DEFAULT_CHANGE_FIELD
            }
            loaded = true
        })
    })

    /** The options as shown, or `null` while a begin-at box is invalid. */
    function shown(): NumberOptions | null {
        if (trackBegin === null) return null
        let disc: DiscOptions | null = null
        if (discOn) {
            if (discBegin === null) return null
            disc = {
                start: discBegin,
                total: discTotal,
                pad: discPad,
                nextOnFolder,
                nextOnField: nextOnFieldOn ? nextOnField : null,
                resetTrack,
                resetDiscOnField: resetDiscOn ? resetDiscField : null,
            }
        }
        return { trackStart: trackBegin, trackTotal, trackPad, disc }
    }

    async function submit(): Promise<void> {
        const options = shown()
        if (options === null || busy) return
        busy = true
        try {
            const ran = await runScoped(async (scope) => {
                await tags.tool({ kind: 'autoNumber', ...options }, scope)
                return tags.error === null
            })
            if (ran === true) await saveNumbering(options)
        } finally {
            busy = false
        }
        onClose()
    }

    function label(key: MessageKey): string {
        return tString(key)
    }
</script>

<ModalDialog
    titleId="tag-numbering-title"
    containerStyle="width: 460px"
    onclose={onClose}
    onkeydown={(e: KeyboardEvent) => {
        if (e.key === 'Enter' && e.target instanceof HTMLInputElement && e.target.type !== 'checkbox' && canSubmit) {
            e.preventDefault()
            void submit()
        }
    }}
>
    {#snippet title()}{tString('multiRename.tags.numbering.title')}{/snippet}
    <div class="numbering">
        {#if trackCount === 1}
            <p class="note" role="status">{tString('multiRename.tags.numbering.singleFile')}</p>
        {/if}
        <fieldset>
            <legend>{tString('multiRename.tags.numbering.track')}</legend>
            <div class="begin">
                <label for="tag-numbering-track-start">{tString('multiRename.tags.numbering.begin')}</label>
                <TextInput
                    id="tag-numbering-track-start"
                    bind:value={trackStart}
                    invalid={trackInvalid}
                    inputmode="numeric"
                    containerStyle="width: 80px"
                    aria-describedby={trackInvalid ? 'tag-numbering-track-error' : undefined}
                />
            </div>
            {#if trackInvalid}
                <p class="error" id="tag-numbering-track-error" role="alert">{tString('multiRename.tags.numbering.invalidBegin')}</p>
            {/if}
            <Checkbox bind:checked={trackTotal}>{label('multiRename.tags.numbering.total')}</Checkbox>
            <Checkbox bind:checked={trackPad}>{label('multiRename.tags.numbering.pad')}</Checkbox>
        </fieldset>
        <fieldset>
            <legend>
                <Checkbox bind:checked={discOn}>{label('multiRename.tags.numbering.disc')}</Checkbox>
            </legend>
            <div class="begin">
                <label for="tag-numbering-disc-start">{tString('multiRename.tags.numbering.begin')}</label>
                <TextInput
                    id="tag-numbering-disc-start"
                    bind:value={discStart}
                    invalid={discInvalid}
                    disabled={!discOn}
                    inputmode="numeric"
                    containerStyle="width: 80px"
                    aria-describedby={discInvalid ? 'tag-numbering-disc-error' : undefined}
                />
            </div>
            {#if discInvalid}
                <p class="error" id="tag-numbering-disc-error" role="alert">{tString('multiRename.tags.numbering.invalidBegin')}</p>
            {/if}
            <Checkbox bind:checked={discTotal} disabled={!discOn}>{label('multiRename.tags.numbering.total')}</Checkbox>
            <Checkbox bind:checked={discPad} disabled={!discOn}>{label('multiRename.tags.numbering.pad')}</Checkbox>
            <Checkbox bind:checked={nextOnFolder} disabled={!discOn}>{label('multiRename.tags.numbering.nextOnFolder')}</Checkbox>
            <div class="change">
                <Checkbox bind:checked={nextOnFieldOn} disabled={!discOn}>{label('multiRename.tags.numbering.nextOnField')}</Checkbox>
                <Select
                    items={FIELD_ITEMS}
                    value={nextOnField}
                    onChange={(value: string) => { nextOnField = TAG_FIELDS.find((f) => f === value) ?? nextOnField }}
                    disabled={!discOn || !nextOnFieldOn}
                    ariaLabel={tString('multiRename.tags.numbering.nextOnField')}
                />
            </div>
            <Checkbox bind:checked={resetTrack} disabled={!discOn}>{label('multiRename.tags.numbering.resetTrack')}</Checkbox>
            <div class="change">
                <Checkbox bind:checked={resetDiscOn} disabled={!discOn}>{label('multiRename.tags.numbering.resetDisc')}</Checkbox>
                <Select
                    items={FIELD_ITEMS}
                    value={resetDiscField}
                    onChange={(value: string) => { resetDiscField = TAG_FIELDS.find((f) => f === value) ?? resetDiscField }}
                    disabled={!discOn || !resetDiscOn}
                    ariaLabel={tString('multiRename.tags.numbering.resetDisc')}
                />
            </div>
        </fieldset>
    </div>
    {#snippet footer()}
        <Button variant="secondary" onclick={onClose}>{tString('multiRename.tags.numbering.cancel')}</Button>
        <Button variant="primary" disabled={!canSubmit} onclick={() => void submit()}>{tString('multiRename.tags.numbering.ok')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .numbering {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    fieldset {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin: 0;
        padding: var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .begin,
    .change {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .note,
    .error {
        margin: 0;
    }

    .note {
        color: var(--color-text-secondary);
    }

    .error {
        color: var(--color-error);
    }
</style>
