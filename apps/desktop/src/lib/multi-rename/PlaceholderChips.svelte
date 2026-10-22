<script lang="ts">
    /**
     * The placeholder buttons beside the masks: `[N]` `[E]` `[P]` `[C]` `[YMD]` insert at the name
     * mask's caret, and `media ▾` (⌘⌥X) opens the field picker (`MediaFieldPicker`), whose picks go
     * in at the caret one after another, a space between them. Each button's tooltip is a
     * `PlaceholderTip`, its examples rendered by the engine.
     */
    import { tick } from 'svelte'
    import Chip from '#lib/ui/Chip.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MediaField } from '#lib/tauri-commands/index.js'
    import type { MultiRenameState } from './multi-rename-state.svelte'
    import { insertAtCaret } from './spec'
    import { MEDIA_HELP, PLACEHOLDER_HELP } from './placeholder-help'
    import PlaceholderTip from './PlaceholderTip.svelte'
    import MediaFieldPicker from './MediaFieldPicker.svelte'
    import { MEDIA_FIELD_LABELS, chipWord, fieldToken, insertField, type Inserted, type PickerExit } from './media-fields'

    interface Props {
        tool: MultiRenameState
        /** The name mask's `<input>`: placeholders go in at its caret. */
        maskInput: HTMLInputElement | undefined
        /** Every tooltip's examples, rendered by the engine; `null` until they come. */
        examples: ReadonlyMap<string, string> | null
        /** Speaks to screen readers through the sheet's live region. */
        announce: (message: string) => void
    }

    const { tool, maskInput, examples, announce }: Props = $props()

    /** Each button's tooltip body, which the tooltip adopts on show. */
    const tipContent = $state<Partial<Record<string, HTMLDivElement>>>({})
    let mediaChip = $state<HTMLButtonElement>()
    let pickerOpen = $state(false)
    /** Fields picked since the picker opened. */
    let picked = $state.raw<ReadonlySet<MediaField>>(new Set())
    /** Where the last pick left the mask, so the next one gets a space before it. */
    let lastPick: Inserted | null = null

    /** Puts `next` in the name mask, its caret after the insert; `focus` hands the field focus too. */
    function writeMask(next: Inserted, focus: boolean): void {
        tool.update({ nameMask: next.mask })
        void tick().then(() => {
            if (focus) maskInput?.focus()
            maskInput?.setSelectionRange(next.caret, next.caret)
        })
    }

    function insertPlaceholder(placeholder: string): void {
        writeMask(insertAtCaret(tool.spec.nameMask, placeholder, maskInput?.selectionStart ?? null), true)
    }

    /** ⌘⌥X and the button: opens the picker, or closes it when it's open. */
    export function togglePicker(): void {
        if (pickerOpen) {
            closePicker('mask')
            return
        }
        picked = new Set()
        lastPick = null
        pickerOpen = true
    }

    function pick(field: MediaField, close: boolean): void {
        const next = insertField({
            mask: tool.spec.nameMask,
            caret: maskInput?.selectionStart ?? null,
            token: fieldToken(field),
            after: lastPick,
        })
        lastPick = next
        picked = new Set([...picked, field])
        writeMask(next, false)
        announce(tString('multiRename.mediaPicker.inserted', { field: tString(MEDIA_FIELD_LABELS[field]) }))
        if (close) closePicker('mask')
    }

    function closePicker(exit: PickerExit): void {
        pickerOpen = false
        lastPick = null
        if (exit === 'mask') void tick().then(() => maskInput?.focus())
        else if (exit !== 'none') focusBeside(exit === 'forward')
    }

    /** Tab / ⇧Tab out of the picker: on to the control after (or before) the button, as Tab from it would. */
    function focusBeside(forward: boolean): void {
        const chip = mediaChip
        const scope = chip?.closest<HTMLElement>('[role="dialog"]')
        if (!chip || !scope) return
        const tabbable = [
            ...scope.querySelectorAll<HTMLElement>(
                'button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
            ),
        ].filter((el) => el.tabIndex >= 0 && el.closest('[hidden]') === null && el.closest('.ui-popover') === null)
        const at = tabbable.indexOf(chip)
        ;(tabbable[at + (forward ? 1 : -1)] ?? chip).focus()
    }
</script>

<div class="placeholders" role="group" aria-label={tString('multiRename.insertPlaceholder')}>
    {#each PLACEHOLDER_HELP as help (help.placeholder)}
        <Chip
            variant="insert"
            size="field"
            mono
            label={help.placeholder}
            tooltipContent={{ contentEl: tipContent[help.placeholder] }}
            onActivate={() => { insertPlaceholder(help.placeholder) }}
        />
        <PlaceholderTip {help} rendered={examples} bind:contentEl={tipContent[help.placeholder]} />
    {/each}
    <Chip
        bind:chipElement={mediaChip}
        variant="filter"
        size="field"
        label={chipWord(tString('multiRename.mediaButton'))}
        ariaLabel={tString('multiRename.mediaButtonName')}
        isOpen={pickerOpen}
        tooltipContent={{ contentEl: tipContent[MEDIA_HELP.placeholder] }}
        onActivate={togglePicker}
    />
    <!-- The tooltip leads with the chip's whole word, which a long translation may cut. -->
    <PlaceholderTip
        help={MEDIA_HELP}
        name={chipWord(tString('multiRename.mediaButton'))}
        rendered={examples}
        bind:contentEl={tipContent[MEDIA_HELP.placeholder]}
    />
</div>
{#if mediaChip}
    <MediaFieldPicker
        anchor={mediaChip}
        open={pickerOpen}
        progress={tool.media}
        loadOverview={tool.mediaOverview}
        {picked}
        onPick={pick}
        onClose={closePicker}
    />
{/if}

<style>
    /* Spans the chip column like a segmented bar: one small gap, and each chip grows from its own
       label's width by an equal share of the spare room (`MultiRenameDialog`'s § Layout). */
    .placeholders {
        justify-self: stretch;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .placeholders > :global(.chip) {
        flex: 1 1 auto;
    }

    /* `media ▾` is a word, and a translation can be three times as long ("đa phương tiện"): the
       chip stops growing the chip column (which would squeeze the masks), and its word ends in an
       ellipsis. The ▾ sits outside the word and never shrinks, so the affordance always shows; the
       accessible name and the tooltip's first line carry the whole word. */
    .placeholders > :global(.chip-filter) {
        max-width: 8em;
    }

    .placeholders > :global(.chip-filter .chip-label) {
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    /* Decoration with no alt text: the chip's `aria-haspopup` already says it opens a list. */
    .placeholders > :global(.chip-filter)::after {
        content: '▾' / '';
        flex-shrink: 0;
    }
</style>
