<script lang="ts">
    /**
     * The `media ▾` button's field picker: a filter box over a listbox of the photo and music
     * fields, in two groups (the one more of the files have first), each with its value from the
     * first file that has it, or a dimmed "none in these files". Focus stays in the filter box;
     * ↑/↓ move the active field (`aria-activedescendant`). It stays open for several picks: a click
     * or Space (with an empty filter; otherwise Space types) picks and keeps it open, Enter picks and closes, Esc closes, Tab / ⇧Tab close and
     * move on. A field picked while it's open carries a ✓. The sheet does the inserting and the
     * announcing (`onPick`), and puts focus where `onClose` says.
     */
    import Popover from '#lib/ui/Popover.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { createDeferredActiveDescendant } from '#lib/ui/deferred-active-descendant.svelte.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import type { MediaField, MediaOverview, MediaProgress } from '#lib/tauri-commands/index.js'
    import { pickerGroups, type PickerExit } from './media-fields'

    interface Props {
        /** The `media ▾` button: the picker hangs under it. */
        anchor: HTMLElement
        open: boolean
        /** The read's progress: a step asks for the overview again, so values fill in while it's open. */
        progress: MediaProgress | null
        loadOverview: () => Promise<MediaOverview | null>
        /** Fields picked since it opened: they carry a ✓. */
        picked: ReadonlySet<MediaField>
        /** Insert `field` at the name mask's caret; `close` for Enter. */
        onPick: (field: MediaField, close: boolean) => void
        onClose: (exit: PickerExit) => void
    }

    const { anchor, open, progress, loadOverview, picked, onPick, onClose }: Props = $props()

    const LIST_ID = 'media-field-picker-list'
    const optionId = (field: MediaField): string => `media-field-${field}`

    let filter = $state('')
    let active = $state<MediaField | null>(null)
    let overview = $state.raw<MediaOverview | null>(null)
    let asked = 0
    const activeDescendant = createDeferredActiveDescendant()

    const groups = $derived(overview ? pickerGroups(overview.fields, filter, tString) : [])
    const shown = $derived(groups.flatMap((group) => group.fields.map((f) => f.field)))
    const done = $derived((overview?.progress.done ?? false) || progress?.done === true)

    // Each open starts fresh: no filter, the first field active.
    $effect(() => {
        if (!open) return
        filter = ''
        active = null
    })

    // On open and on each step of the read while open; a newer ask overtakes an older answer.
    const overviewAsk = $derived(open ? { progress } : null)
    $effect(() => {
        if (overviewAsk === null) return
        const mine = ++asked
        void loadOverview().then((answer) => {
            if (mine === asked && answer !== null) overview = answer
        })
    })

    // The active field stays among the shown ones: the first after a filter change.
    $effect(() => {
        if (active === null || !shown.includes(active)) active = shown.at(0) ?? null
    })

    function move(step: number): void {
        if (shown.length === 0) return
        const at = active === null ? -1 : shown.indexOf(active)
        active = shown[Math.max(0, Math.min(shown.length - 1, at + step))]
        document.getElementById(optionId(active))?.scrollIntoView({ block: 'nearest' })
    }

    function onKeydown(e: KeyboardEvent): void {
        if (e.isComposing) return
        switch (e.key) {
            case 'ArrowDown':
            case 'ArrowUp':
                claimKey(e)
                move(e.key === 'ArrowDown' ? 1 : -1)
                return
            case ' ':
                // Space picks only from an empty filter; once there's text it types, so "album a" filters.
                if (filter !== '') return
                claimKey(e)
                if (active !== null) onPick(active, false)
                return
            case 'Enter':
                claimKey(e)
                if (active !== null) onPick(active, true)
                return
            case 'Escape':
                claimKey(e)
                onClose('mask')
                return
            case 'Tab':
                claimKey(e)
                onClose(e.shiftKey ? 'backward' : 'forward')
                return
        }
    }
</script>

<Popover {anchor} {open} onClose={() => { onClose('none') }} ariaLabel={tString('multiRename.mediaPicker.title')} surface="solid">
    <div class="picker">
        <TextInput
            value={filter}
            oninput={(e: Event) => { filter = (e.currentTarget as HTMLInputElement).value }}
            ariaLabel={tString('multiRename.mediaPicker.filter')}
            placeholder={tString('multiRename.mediaPicker.filter')}
            leadingIcon="search"
            role="combobox"
            aria-expanded="true"
            aria-controls={LIST_ID}
            aria-autocomplete="list"
            aria-activedescendant={activeDescendant.of(active === null ? undefined : optionId(active))}
            onfocus={() => { activeDescendant.tookFocus(active === null ? undefined : optionId(active)) }}
            onblur={() => { activeDescendant.reset() }}
            onkeydown={onKeydown}
        />
        <div class="list" id={LIST_ID} role="listbox" aria-label={tString('multiRename.mediaPicker.title')}>
            {#each groups as group (group.source)}
                <div role="group" aria-label={tString(group.heading)}>
                    <div class="heading" aria-hidden="true">{tString(group.heading)}</div>
                    {#each group.fields as option (option.field)}
                        <!-- Mousedown keeps focus in the filter box; the click picks and keeps it open. -->
                        <div
                            id={optionId(option.field)}
                            class="option"
                            class:is-active={option.field === active}
                            role="option"
                            tabindex="-1"
                            aria-selected={option.field === active}
                            onmousedown={(e: MouseEvent) => { e.preventDefault() }}
                            onclick={() => { active = option.field; onPick(option.field, false) }}
                            onkeydown={onKeydown}
                        >
                            <span class="name">{tString(option.label)}</span>
                            <span class="value" class:is-none={option.example === null}>
                                {option.example ??
                                    tString(done ? 'multiRename.mediaPicker.none' : 'multiRename.mediaPicker.noneYet')}
                            </span>
                            <span class="mark">
                                {#if picked.has(option.field)}
                                    <Icon name="check" size={12} aria-hidden="true" />
                                    <span class="sr-only">{tString('multiRename.mediaPicker.insertedMark')}</span>
                                {/if}
                            </span>
                        </div>
                    {/each}
                </div>
            {/each}
        </div>
        {#if overview !== null && groups.length === 0}
            <p class="no-match">{tString('multiRename.mediaPicker.noMatch', { filter })}</p>
        {/if}
    </div>
</Popover>

<style>
    .picker {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 360px;
    }

    .list {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .heading {
        padding: 0 var(--spacing-sm) var(--spacing-xxs);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        font-weight: 600;
    }

    /* Name, value, and the ✓: three columns, so the values line up down the list. */
    .option {
        display: grid;
        grid-template-columns: 9em minmax(0, 1fr) 12px;
        gap: var(--spacing-sm);
        align-items: baseline;
        padding: var(--spacing-xxs) var(--spacing-sm);
        border-radius: var(--radius-sm);
    }

    .option.is-active {
        background: var(--color-accent-subtle);
    }

    .value {
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
        color: var(--color-text-secondary);
    }

    .value.is-none {
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .mark {
        display: flex;
        align-self: center;
        color: var(--color-accent-text);
    }

    .no-match {
        margin: 0;
        padding: 0 var(--spacing-sm);
        color: var(--color-text-secondary);
    }
</style>
