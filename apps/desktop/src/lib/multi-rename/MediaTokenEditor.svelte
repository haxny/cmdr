<script lang="ts">
    /**
     * The media field token's editor (`[=exif.taken:d]`): the field's format by its kind (a date's
     * letters, with a few presets; a number's digits; a text's characters, as a range), the
     * `|modified` fallback for a date, how the token fares across the files, and what it gives on
     * the files around the list's cursor, rendered by the engine (`get_multi_rename_field_samples`).
     * Every edit goes straight to `onChange`, which rewrites the token; a half-typed format waits.
     *
     * Keys: Enter, and ArrowUp from the first field, leave through `onDone` (claimed, so the sheet's
     * Enter-starts never sees them). Escape belongs to the `Popover` around this.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import TextInput from '#lib/ui/TextInput.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import ToggleGroup, { type ToggleGroupOption } from '#lib/ui/ToggleGroup.svelte'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import type { FieldGap, FieldSamples } from '#lib/tauri-commands/index.js'
    import { MEDIA_FIELDS, formatMediaToken, parseMediaToken, type MediaToken } from './media-token'
    import { MEDIA_FIELD_LABELS } from './media-fields'
    import { rangeSampleSource } from './range-sample-source'
    import type { TokenEditorProps } from './mask-token-kinds'

    const { value, onChange, onDone }: TokenEditorProps<MediaToken> = $props()

    const source = rangeSampleSource()
    const uid = $props.id()
    const hintId = `media-editor-hint-${uid}`
    const invalidId = `media-editor-invalid-${uid}`

    const kind = $derived(MEDIA_FIELDS[value.field].kind)
    const tokenText = $derived(formatMediaToken(value))
    /** The format as the token writes it after its `:`, or `''` for none. */
    const formatText = $derived(tokenText.includes(':') ? tokenText.slice(tokenText.indexOf(':') + 1) : '')

    /** The date presets: none (date and time), the date, the time, and both run together. */
    const DATE_PRESETS: { format: string; label: string }[] = [
        { format: '', label: tString('multiRename.mediaEditor.preset.dateTime') },
        { format: 'd', label: tString('multiRename.mediaEditor.preset.date') },
        { format: 't', label: tString('multiRename.mediaEditor.preset.time') },
        { format: 'YMD_hms', label: tString('multiRename.mediaEditor.preset.compact') },
    ]
    const CUSTOM = 'custom'
    const presetOptions: ToggleGroupOption[] = DATE_PRESETS.map(({ format, label }) => ({
        value: format === '' ? 'none' : format,
        label,
    }))

    // Settled `$state`, never a derived of the token: `ToggleGroup`'s teardown reads its props one
    // last time (the range editor's DETAILS § Range editor gotcha).
    let preset = $state(CUSTOM)
    $effect.pre(() => {
        const found = DATE_PRESETS.find(({ format }) => format === formatText)
        preset = found ? (found.format === '' ? 'none' : found.format) : CUSTOM
    })

    /** What the user typed in the format box, kept until it leaves: it may not be a format yet. */
    let draft = $state<string | null>(null)

    /** Writes `text` as the format when the grammar takes it (empty is none); a half-typed one writes nothing. */
    function writeFormat(text: string): void {
        const parsed = parseFormat(text)
        if (parsed !== null) onChange(parsed)
    }

    function parseFormat(text: string): MediaToken | null {
        const trimmed = text.trim()
        const bar = value.fallback ? '|modified' : ''
        return trimmed === ''
            ? { ...value, format: null }
            : parseMediaToken(`=${MEDIA_FIELDS[value.field].name}${bar}:${trimmed}`)
    }

    /** A Digits box whose text the grammar can't read (`0`, `abc`): said under the box, not ignored. */
    const digitsInvalid = $derived(kind === 'number' && draft !== null && parseFormat(draft) === null)

    function typeFormat(text: string): void {
        draft = text
        writeFormat(text)
    }

    function choosePreset(next: string): void {
        draft = null
        writeFormat(next === 'none' ? '' : next)
    }

    function setFallback(on: boolean): void {
        onChange({ ...value, fallback: on })
    }

    function onKeydown(e: KeyboardEvent, first: boolean): void {
        if (e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        if (e.key === 'Enter' || (first && e.key === 'ArrowUp')) {
            claimKey(e)
            onDone()
        }
    }

    /** The engine's latest answer, for the rows around the list's cursor. */
    let answer = $state.raw<{ cursor: number; samples: FieldSamples } | null>(null)
    let asked = 0

    $effect(() => {
        // Asks again for a new token and for every new preview (a new cursor row, reads landing).
        const rows = source?.sampleRows
        const token = tokenText
        if (!source || !rows) return
        const id = ++asked
        void source.fieldSamples(token, rows.rows).then((samples) => {
            // A newer question overtook this one, or the engine couldn't say: keep what's shown.
            if (id !== asked || samples === null) return
            answer = { cursor: rows.cursor, samples }
        })
    })

    function gapWords(gap: FieldGap | null): string {
        switch (gap?.type) {
            case 'reading':
                return tString('multiRename.mediaEditor.gap.reading')
            case 'unreadable':
                return tString('multiRename.mediaEditor.gap.unreadable')
            default:
                return tString('multiRename.mediaEditor.gap.noValue')
        }
    }

    const usage = $derived(answer?.samples.usage ?? null)
    const usageLine = $derived.by(() => {
        if (!usage) return null
        if (value.fallback && usage.fallback > 0) {
            return tString('multiRename.mediaEditor.usesModified', {
                count: usage.fallback,
                countText: formatInteger(usage.fallback),
            })
        }
        if (!value.fallback && usage.missing > 0) {
            return tString('multiRename.mediaEditor.missing', { count: usage.missing, countText: formatInteger(usage.missing) })
        }
        return null
    })
    const readingLine = $derived(
        usage && usage.reading > 0
            ? tString('multiRename.mediaEditor.stillReading', {
                  count: usage.reading,
                  countText: formatInteger(usage.reading),
              })
            : null,
    )

    const formatLabel = $derived(
        kind === 'date'
            ? tString('multiRename.mediaEditor.format')
            : kind === 'number'
              ? tString('multiRename.mediaEditor.digits')
              : tString('multiRename.mediaEditor.characters'),
    )
    const formatHint = $derived(
        kind === 'date'
            ? tString('multiRename.mediaEditor.dateHint')
            : kind === 'number'
              ? tString('multiRename.mediaEditor.digitsHint')
              : tString('multiRename.mediaEditor.charactersHint'),
    )
</script>

<div class="media-editor">
    <p class="field-name">{tString(MEDIA_FIELD_LABELS[value.field])}</p>
    <!-- The format box first: the editor's first stop, where ArrowUp leaves for the mask. -->
    <label class="format">
        <span class="label">{formatLabel}</span>
        <TextInput
            value={draft ?? formatText}
            mono
            radius="md"
            ariaLabel={formatLabel}
            aria-describedby={digitsInvalid ? `${hintId} ${invalidId}` : hintId}
            aria-invalid={digitsInvalid ? 'true' : undefined}
            oninput={(e: Event) => { typeFormat((e.currentTarget as HTMLInputElement).value) }}
            onblur={() => { draft = null }}
            onkeydown={(e: KeyboardEvent) => { onKeydown(e, true) }}
        />
    </label>
    <p class="hint" id={hintId}>{formatHint}</p>
    {#if digitsInvalid}
        <p class="invalid" id={invalidId} role="alert">{tString('multiRename.mediaEditor.digitsInvalid')}</p>
    {/if}
    {#if kind === 'date'}
        <ToggleGroup
            semantics="toggles"
            value={preset}
            options={presetOptions}
            onChange={choosePreset}
            ariaLabel={tString('multiRename.mediaEditor.presets')}
        />
        <span class="fallback">
            <Checkbox checked={value.fallback} onCheckedChange={setFallback}>
                {tString('multiRename.mediaEditor.fallback')}
            </Checkbox>
        </span>
    {/if}
    {#if usageLine || readingLine}
        <p class="usage" aria-live="polite">
            {#if usageLine}<span>{usageLine}</span>{/if}
            {#if readingLine}<span>{readingLine}</span>{/if}
        </p>
    {/if}
    {#if answer && answer.samples.samples.length > 0}
        <div class="samples">
            <span class="label">{tString('multiRename.rangeEditor.onYourFiles')}</span>
            <ul>
                {#each answer.samples.samples as sample (sample.row)}
                    <li aria-current={sample.row === answer.cursor ? 'true' : undefined}>
                        {#if sample.text !== null}
                            <span class="value">{sample.text}</span>
                        {:else}
                            <span class="gap" class:is-missing={sample.gap?.type !== 'reading'}>{gapWords(sample.gap)}</span>
                        {/if}
                    </li>
                {/each}
            </ul>
        </div>
    {/if}
</div>

<style>
    .media-editor {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 320px;
    }

    p {
        margin: 0;
    }

    .field-name {
        font-weight: 600;
    }

    .format {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .label,
    .hint,
    .usage {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .invalid {
        margin: 0;
        color: var(--color-error-text);
        font-size: var(--font-size-sm);
    }

    .usage {
        display: flex;
        flex-direction: column;
    }

    .fallback {
        display: flex;
    }

    .samples {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    ul {
        margin: 0;
        padding: 0;
        list-style: none;
        font-family: var(--font-mono);
    }

    li {
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    /* The cursor's file, as the range editor sets it apart. */
    li[aria-current='true'] {
        background: var(--color-accent-subtle);
    }

    .gap {
        color: var(--color-text-tertiary);
        font-family: var(--font-system);
        font-style: italic;
    }

    .gap.is-missing {
        color: var(--color-warning-text);
    }
</style>
