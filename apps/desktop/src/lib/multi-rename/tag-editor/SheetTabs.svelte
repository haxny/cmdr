<script lang="ts">
    /**
     * The tab strip of the Multi-Rename sheet: Rename (today's sheet) and Tags. Only mounted when the
     * `multiRename.tagEditor` setting is on. ←/→ and Home/End move between the tabs (roving tabindex), ⌘1 and ⌘2
     * (registry commands) pick one from anywhere in the sheet.
     */
    import { onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import { TAB_COMMANDS, type SheetTab } from './sheet-tab'

    interface Props {
        active: SheetTab
        onSelect: (tab: SheetTab) => void
    }

    const { active, onSelect }: Props = $props()

    const TABS: { id: SheetTab; labelKey: 'multiRename.tabs.rename' | 'multiRename.tabs.tags'; chip: string }[] = [
        { id: 'rename', labelKey: 'multiRename.tabs.rename', chip: '⌘1' },
        { id: 'tags', labelKey: 'multiRename.tabs.tags', chip: '⌘2' },
    ]

    let tablist = $state<HTMLElement>()

    function select(tab: SheetTab, focus: boolean): void {
        onSelect(tab)
        if (focus) tablist?.querySelector<HTMLElement>(`#sheet-tab-${tab}`)?.focus()
    }

    function onTablistKeydown(e: KeyboardEvent): void {
        if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        const at = TABS.findIndex(({ id }) => id === active)
        const target =
            e.key === 'ArrowRight' ? at + 1 : e.key === 'ArrowLeft' ? at - 1 : e.key === 'Home' ? 0 : e.key === 'End' ? TABS.length - 1 : null
        if (target === null) return
        claimKey(e)
        select(TABS[(target + TABS.length) % TABS.length].id, true)
    }

    // ⌘1 / ⌘2 would otherwise reach the main window's view-mode keys.
    onMount(() => {
        const onWindowKeydown = (e: KeyboardEvent): void => {
            if (e.isComposing) return
            const match = TAB_COMMANDS.find(({ command }) => eventMatchesCommand(e, command))
            if (!match) return
            claimKey(e)
            // A box still in focus commits its text first, as a click on the tab strip would.
            const focused = document.activeElement
            if (focused instanceof HTMLElement && focused.closest('#sheet-panel-tags')) focused.blur()
            select(match.tab, false)
        }
        window.addEventListener('keydown', onWindowKeydown, true)
        return () => {
            window.removeEventListener('keydown', onWindowKeydown, true)
        }
    })
</script>

<div
    class="tabs"
    role="tablist"
    tabindex="-1"
    aria-label={tString('multiRename.tabs.label')}
    bind:this={tablist}
    onkeydown={onTablistKeydown}
>
    {#each TABS as tab (tab.id)}
        <button
            type="button"
            role="tab"
            id="sheet-tab-{tab.id}"
            class="tab"
            class:active={active === tab.id}
            aria-selected={active === tab.id}
            aria-controls={active === tab.id ? `sheet-panel-${tab.id}` : undefined}
            tabindex={active === tab.id ? 0 : -1}
            onclick={() => { select(tab.id, false) }}
        >
            {tString(tab.labelKey)}
            <span class="chip" aria-hidden="true">{tab.chip}</span>
        </button>
    {/each}
</div>

<style>
    .tabs {
        display: flex;
        gap: var(--spacing-xs);
        padding: 0 var(--spacing-md);
        border-bottom: 1px solid var(--color-border-subtle);
        flex-shrink: 0;
    }

    .tab {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-md);
        border: none;
        border-bottom: 2px solid transparent;
        background: none;
        color: var(--color-text-secondary);
        font: inherit;
        cursor: default;
    }

    .tab.active {
        color: var(--color-text-primary);
        border-bottom-color: var(--color-accent);
    }

    .tab:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .chip {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
