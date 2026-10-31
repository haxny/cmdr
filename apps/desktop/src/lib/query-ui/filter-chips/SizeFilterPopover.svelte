<script lang="ts">
    /**
     * Size popover body: a multi-column list-style grid. Col 1 = comparator (`any` / `≥` / `≤` /
     * `between`). Col 2 = numeric preset (`0` / `1` / `5` / ... / `Custom…`). Col 3 = unit
     * (`bytes` / `KB` / `MB` / `GB`). When col 1 = `between`, cols 4 + 5 mirror cols 2 + 3 for the
     * upper bound. Value + unit cells stay clickable while the comparator is `any`; they render
     * dimmed and a click auto-promotes the comparator to `gte` and applies the clicked value.
     *
     * Extracted from `FilterChips.svelte`. The parent owns the chip strip, the `openChip` state,
     * and the ⌥S opener; this component owns the popover surface and its local custom-input flags.
     * The `FilterPopover` shell (positioning, focus trap, Esc-scoped close, labelled header) is
     * wrapped here so the parent only threads `anchor` / `open` / `onClose`.
     */
    import { tick } from 'svelte'
    import FilterPopover from '#lib/ui/FilterPopover.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { SizeFilter, SizeUnit } from '../query-filter-state.svelte'
    import { SIZE_PRESETS, byteUnitLabel, isSizeRangeDisabled, showsUpperBound } from './filter-popover-helpers'
    import { unitLabel } from '#lib/units/index.js'
    import { getFileSizeFormat } from '#lib/settings/reactive-settings.svelte.js'
    import './filter-popover.css'

    /** The unit cells after the kilobyte, in the order the column lists them. */
    const LARGER_UNITS = ['MB', 'GB'] as const

    interface Props {
        /** The Size chip element, used by the popover shell for positioning + focus return. */
        anchor: HTMLElement
        /** Whether the popover is shown (owned by the parent's `openChip` state). */
        open: boolean
        /** Fired when the popover wants to close (Esc / click outside). */
        onClose: () => void
        sizeFilter: SizeFilter
        sizeValue: string
        sizeUnit: SizeUnit
        sizeValueMax: string
        sizeUnitMax: SizeUnit
        setSizeFilter: (v: SizeFilter) => void
        setSizeValue: (v: string) => void
        setSizeUnit: (v: SizeUnit) => void
        setSizeValueMax: (v: string) => void
        setSizeUnitMax: (v: SizeUnit) => void
        onInput: (setter: (v: string) => void, search?: boolean) => (e: Event) => void
        scheduleSearch: () => void
    }

    const {
        anchor,
        open,
        onClose,
        sizeFilter,
        sizeValue,
        sizeUnit,
        sizeValueMax,
        sizeUnitMax,
        setSizeFilter,
        setSizeValue,
        setSizeUnit,
        setSizeValueMax,
        setSizeUnitMax,
        onInput,
        scheduleSearch,
    }: Props = $props()

    /**
     * The comparator column (col 1) of the Size popover. The `≥` and `≤` use
     * the math glyphs so the rendered cell reads cleanly rather than the ASCII soup.
     */
    const SIZE_COMPARATORS = $derived<ReadonlyArray<{ value: SizeFilter; label: string }>>([
        { value: 'any', label: tString('queryUi.size.comparator.any') },
        { value: 'gte', label: '≥' },
        { value: 'lte', label: '≤' },
        { value: 'eq', label: '=' },
        { value: 'between', label: tString('queryUi.size.comparator.between') },
    ])

    /**
     * Whether the user has flipped the value column into "Custom…" mode for the
     * lower / upper bound. The free-form `<input>` only appears while these are
     * `true`; the inline `setSizeValue('')` zaps the existing preset selection
     * so the radio-set goes back to "no preset highlighted".
     */
    let sizeIsCustomLower = $state(false)
    let sizeIsCustomUpper = $state(false)
    let customLowerInput = $state<HTMLInputElement | undefined>()
    let customUpperInput = $state<HTMLInputElement | undefined>()

    /** Re-sync the "custom" flag against an externally-set value (AI mode, MCP prefill). */
    $effect(() => {
        // If the dialog or AI lands a value that exactly matches a preset, drop out of custom mode
        // so the preset row lights up; otherwise stay in custom (the input drives the value).
        if (sizeValue && !SIZE_PRESETS.includes(sizeValue)) {
            sizeIsCustomLower = true
        } else if (sizeValue) {
            sizeIsCustomLower = false
        }
    })
    $effect(() => {
        if (sizeValueMax && !SIZE_PRESETS.includes(sizeValueMax)) {
            sizeIsCustomUpper = true
        } else if (sizeValueMax) {
            sizeIsCustomUpper = false
        }
    })

    /**
     * When the user picks a new comparator we schedule a search so the chip
     * label updates immediately.
     */
    function onSizeComparatorEdit(): void {
        scheduleSearch()
    }

    /**
     * Helpers for the "click on disabled cell auto-promotes the comparator"
     * behaviour. When the user clicks a value in the value column (or a unit in the
     * unit column) while the comparator is `any`, promote the comparator to `gte`
     * AND apply the clicked value.
     */
    function pickSizeValue(value: string): void {
        if (sizeFilter === 'any') setSizeFilter('gte')
        setSizeValue(value)
        scheduleSearch()
    }
    function pickSizeUnit(unit: SizeUnit): void {
        if (sizeFilter === 'any') setSizeFilter('gte')
        setSizeUnit(unit)
        scheduleSearch()
    }
    /** One press picks Custom AND puts the caret in the field the press just brought up. */
    async function pickSizeCustomLower(): Promise<void> {
        if (sizeFilter === 'any') setSizeFilter('gte')
        sizeIsCustomLower = true
        setSizeValue('')
        scheduleSearch()
        await tick()
        customLowerInput?.focus()
    }
    async function pickSizeCustomUpper(): Promise<void> {
        sizeIsCustomUpper = true
        setSizeValueMax('')
        scheduleSearch()
        await tick()
        customUpperInput?.focus()
    }
</script>

<!-- Size popover: multi-column list-style grid. Col 1 = comparator (`any` /
     `>=` / `<=` / `between`). Col 2 = numeric preset (`0` / `1` / `5` / ... /
     `Custom…`). Col 3 = unit (`bytes` / `KB` / `MB` / `GB`). When col 1 =
     `between`, cols 4 + 5 mirror cols 2 + 3 for the upper bound. Cols 2-5 render
     disabled when col 1 = `any` (no range to apply). -->
<FilterPopover
    {anchor}
    {open}
    {onClose}
    label={tString('queryUi.size.popover.label')}
    ariaLabel={tString('queryUi.size.popover.aria')}
    sectionClass="size-grid-section"
>
        <div
            class="list-grid"
            class:has-upper={showsUpperBound(sizeFilter)}
            role="group"
            aria-label={tString('queryUi.size.popover.aria')}
        >
            <!-- Col 1: comparator -->
            <div class="list-col" role="radiogroup" aria-label={tString('queryUi.size.aria.comparator')}>
                {#each SIZE_COMPARATORS as opt (opt.value)}
                    <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Grid-style popover cell: each column is a dense list of preset cells (the Custom one turns into an inline field), which a RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                    <button
                        type="button"
                        class="list-cell"
                        class:is-selected={sizeFilter === opt.value}
                        role="radio"
                        aria-checked={sizeFilter === opt.value}
                        onclick={() => {
                            setSizeFilter(opt.value)
                            onSizeComparatorEdit()
                        }}
                    >
                        {opt.label}
                    </button>
                {/each}
            </div>

            <!-- Col 2: lower-bound value. Cells stay clickable when the
                 comparator is `any`; clicking promotes the comparator to
                 `gte` and applies the chosen value. -->
            <div class="list-col" role="radiogroup" aria-label={tString('queryUi.size.aria.minValue')}>
                {#each SIZE_PRESETS as preset (preset)}
                    <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Grid-style popover cell: each column is a dense list of preset cells (the Custom one turns into an inline field), which a RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                    <button
                        type="button"
                        class="list-cell"
                        class:is-selected={sizeValue === preset && !sizeIsCustomLower}
                        class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                        role="radio"
                        aria-checked={sizeValue === preset && !sizeIsCustomLower}
                        onclick={() => {
                            pickSizeValue(preset)
                        }}
                    >
                        {preset}
                    </button>
                {/each}
                <!-- The Custom cell is a button until it's picked, or the AI / history set
                     a custom value (the `sizeIsCustomLower` effect above). Then it becomes a
                     label holding the field: a button's children are presentational, and the
                     label keeps a press on the cell's padding focusing the field. -->
                {#if sizeIsCustomLower}
                    <label class="list-cell list-cell-custom is-selected" class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}>
                        <input
                            bind:this={customLowerInput}
                            type="number"
                            class="popover-input size-input custom-input-inline"
                            value={sizeValue}
                            oninput={onInput(setSizeValue)}
                            aria-label={tString('queryUi.size.aria.customMin')}
                            min="0"
                            step="any"
                            placeholder={tString('queryUi.size.customInputPlaceholder')}
                        />
                    </label>
                {:else}
                    <button
                        type="button"
                        class="list-cell list-cell-custom"
                        class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                        onclick={() => void pickSizeCustomLower()}
                    >
                        {tString('queryUi.size.customCell')}
                    </button>
                {/if}
            </div>

            <!-- Col 3: lower-bound unit -->
            <div class="list-col" role="radiogroup" aria-label={tString('queryUi.size.aria.minUnit')}>
                <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Grid-style popover cell: each column is a dense list of preset cells (the Custom one turns into an inline field), which a RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                <button
                    type="button"
                    class="list-cell"
                    class:is-selected={sizeUnit === 'B'}
                    class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                    role="radio"
                    aria-checked={sizeUnit === 'B'}
                    onclick={() => {
                        pickSizeUnit('B')
                    }}
                >
                    {byteUnitLabel(sizeValue)}
                </button>
                <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Grid-style popover cell: each column is a dense list of preset cells (the Custom one turns into an inline field), which a RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                <button
                    type="button"
                    class="list-cell"
                    class:is-selected={sizeUnit === 'KB'}
                    class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                    role="radio"
                    aria-checked={sizeUnit === 'KB'}
                    onclick={() => {
                        pickSizeUnit('KB')
                    }}
                >
                    {unitLabel('kB', getFileSizeFormat())}
                </button>
                {#each LARGER_UNITS as larger (larger)}
                    <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Grid-style popover cell: each column is a dense list of preset cells (the Custom one turns into an inline field), which a RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                    <button
                        type="button"
                        class="list-cell"
                        class:is-selected={sizeUnit === larger}
                        class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                        role="radio"
                        aria-checked={sizeUnit === larger}
                        onclick={() => {
                            pickSizeUnit(larger)
                        }}
                    >
                        {unitLabel(larger, getFileSizeFormat())}
                    </button>
                {/each}
            </div>

            {#if showsUpperBound(sizeFilter)}
                <!-- Col 4: upper-bound value, with the same Custom cell. -->
                <div class="list-col" role="radiogroup" aria-label={tString('queryUi.size.aria.maxValue')}>
                    {#each SIZE_PRESETS as preset (preset)}
                        <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Grid-style popover cell: each column is a dense list of preset cells (the Custom one turns into an inline field), which a RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                        <button
                            type="button"
                            class="list-cell"
                            class:is-selected={sizeValueMax === preset && !sizeIsCustomUpper}
                            role="radio"
                            aria-checked={sizeValueMax === preset && !sizeIsCustomUpper}
                            onclick={() => {
                                setSizeValueMax(preset)
                                scheduleSearch()
                            }}
                        >
                            {preset}
                        </button>
                    {/each}
                    {#if sizeIsCustomUpper}
                        <label class="list-cell list-cell-custom is-selected">
                            <input
                                bind:this={customUpperInput}
                                type="number"
                                class="popover-input size-input custom-input-inline"
                                value={sizeValueMax}
                                oninput={onInput(setSizeValueMax)}
                                aria-label={tString('queryUi.size.aria.customMax')}
                                min="0"
                                step="any"
                                placeholder={tString('queryUi.size.customInputPlaceholder')}
                            />
                        </label>
                    {:else}
                        <button
                            type="button"
                            class="list-cell list-cell-custom"
                            onclick={() => void pickSizeCustomUpper()}
                        >
                            {tString('queryUi.size.customCell')}
                        </button>
                    {/if}
                </div>

                <!-- Col 5: upper-bound unit -->
                <div class="list-col" role="radiogroup" aria-label={tString('queryUi.size.aria.maxUnit')}>
                    <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Grid-style popover cell: each column is a dense list of preset cells (the Custom one turns into an inline field), which a RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                    <button
                        type="button"
                        class="list-cell"
                        class:is-selected={sizeUnitMax === 'B'}
                        role="radio"
                        aria-checked={sizeUnitMax === 'B'}
                        onclick={() => {
                            setSizeUnitMax('B')
                            scheduleSearch()
                        }}
                    >
                        {byteUnitLabel(sizeValueMax)}
                    </button>
                    <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Grid-style popover cell: each column is a dense list of preset cells (the Custom one turns into an inline field), which a RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                    <button
                        type="button"
                        class="list-cell"
                        class:is-selected={sizeUnitMax === 'KB'}
                        role="radio"
                        aria-checked={sizeUnitMax === 'KB'}
                        onclick={() => {
                            setSizeUnitMax('KB')
                            scheduleSearch()
                        }}
                    >
                        {unitLabel('kB', getFileSizeFormat())}
                    </button>
                    {#each LARGER_UNITS as larger (larger)}
                        <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Grid-style popover cell: each column is a dense list of preset cells (the Custom one turns into an inline field), which a RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                        <button
                            type="button"
                            class="list-cell"
                            class:is-selected={sizeUnitMax === larger}
                            role="radio"
                            aria-checked={sizeUnitMax === larger}
                            onclick={() => {
                                setSizeUnitMax(larger)
                                scheduleSearch()
                            }}
                        >
                            {unitLabel(larger, getFileSizeFormat())}
                        </button>
                    {/each}
                </div>
            {/if}
        </div>
</FilterPopover>

<style>
    .size-input {
        width: 80px;
    }
</style>
