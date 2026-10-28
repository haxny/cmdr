<script lang="ts">
    /**
     * The Tags window's function-key bar, at its foot: the main window's `FunctionKeyBar` look (one flat band, a `<kbd>`
     * chip and a label per button, Shift showing the Shift row), with the tag commands' own slots
     * (`tags-function-keys.ts`). Each chip is the command's live binding, so a rebind in Settings shows at once. It
     * never takes focus: every key path is the key itself, and a press keeps focus where it was.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import {
        getFirstShiftShortcutReactive,
        getFirstShortcutReactive,
    } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
    import { toAriaKeyShortcut, toPlatformShortcut } from '#lib/shortcuts/key-capture.js'
    import { TAGS_PLAIN_ROW, TAGS_SHIFT_ROW, TAGS_WRITE_SLOT, type TagsBarCommandId } from './tags-function-keys'

    interface Props {
        /** The commands that can act now; a slot whose command isn't here is disabled. */
        enabled: ReadonlySet<TagsBarCommandId>
        onCommand: (id: TagsBarCommandId) => void
    }

    const { enabled, onCommand }: Props = $props()

    /** Slot 0 is F2, so an empty Shift slot `i` shows `⇧F${i + FIRST_FN_KEY}`. */
    const FIRST_FN_KEY = 2

    interface Slot {
        id: TagsBarCommandId
        label: string
        enabled: boolean
        shortcut: string | undefined
    }

    let shiftHeld = $state(false)

    const toSlot = (id: TagsBarCommandId, label: string, shortcut: string | undefined): Slot => ({
        id,
        label,
        enabled: enabled.has(id),
        shortcut,
    })

    const writeSlot = $derived(
        toSlot(TAGS_WRITE_SLOT.id, tString(TAGS_WRITE_SLOT.label), getFirstShortcutReactive(TAGS_WRITE_SLOT.id)),
    )
    const plainRow = $derived<(Slot | null)[]>([
        ...TAGS_PLAIN_ROW.map((slot) => toSlot(slot.id, tString(slot.label), getFirstShortcutReactive(slot.id))),
        writeSlot,
    ])
    const shiftRow = $derived<(Slot | null)[]>([
        ...TAGS_SHIFT_ROW.map((slot) =>
            slot === null ? null : toSlot(slot.id, tString(slot.label), getFirstShiftShortcutReactive(slot.id)),
        ),
        writeSlot,
    ])

    function onKey(e: KeyboardEvent, down: boolean): void {
        if (e.key === 'Shift') shiftHeld = down
    }
</script>

<svelte:document onkeydown={(e: KeyboardEvent) => { onKey(e, true) }} onkeyup={(e: KeyboardEvent) => { onKey(e, false) }} />
<svelte:window onblur={() => { shiftHeld = false }} />

<!-- The label is the accessible name; the chip reaches a screen reader through `aria-keyshortcuts`. -->
{#snippet commandButton(slot: Slot)}
    <button
        onclick={() => { onCommand(slot.id) }}
        disabled={!slot.enabled}
        tabindex={-1}
        aria-keyshortcuts={slot.shortcut ? toAriaKeyShortcut(slot.shortcut) : undefined}
    >
        {#if slot.shortcut}<kbd aria-hidden="true">{slot.shortcut}</kbd>{/if}<span>{slot.label}</span>
    </button>
{/snippet}

<!-- An F-key with no Shift action: presentational, named by its bare key. -->
{#snippet emptySlot(index: number)}
    {@const chip = toPlatformShortcut(`⇧F${String(index + FIRST_FN_KEY)}`)}
    <!-- The name holds the chip it shows (label in name), unlike the main bar's bare-key name. -->
    <button disabled tabindex={-1} aria-label={tString('fileExplorer.functionKeyBar.noShiftAction', { fnKey: chip })}>
        <kbd>{chip}</kbd><span></span>
    </button>
{/snippet}

<!-- Every button's keyboard path is its key, so the bar never takes focus (as the main window's bar). -->
<!-- svelte-ignore a11y_interactive_supports_focus -->
<div
    class="tags-function-key-bar"
    data-row={shiftHeld ? 'shift' : 'plain'}
    role="toolbar"
    aria-label={tString('multiRename.tags.keys.label')}
    onmousedown={(e) => { e.preventDefault() }}
>
    <!-- eslint-disable @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
    {#each shiftHeld ? shiftRow : plainRow as slot, index (index)}
        {#if slot}
            {@render commandButton(slot)}
        {:else}
            {@render emptySlot(index)}
        {/if}
    {/each}
    <!-- eslint-enable @typescript-eslint/no-confusing-void-expression -->
</div>

<style>
    /* The main window's `FunctionKeyBar` look, rule for rule (that bar is wired to the file commands, so it isn't
       reused). */
    .tags-function-key-bar {
        display: flex;
        flex-shrink: 0;
        background: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border);
    }

    button {
        flex: 1;
        min-width: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
        border: none;
        border-right: 1px solid var(--color-border-subtle);
        background: transparent;
        padding: var(--spacing-sm) 0;
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        transition: background-color var(--transition-fast);
    }

    button > span {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        min-width: 0;
    }

    button:last-child {
        border-right: none;
    }

    button:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
    }

    button:disabled {
        opacity: 0.4;
        cursor: default;
    }

    kbd {
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        padding: 1px var(--spacing-xs);
        white-space: nowrap;
        flex-shrink: 0;
    }
</style>
