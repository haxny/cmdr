<script lang="ts">
    /**
     * The favorites menu (⌃D): the pane's favorites as a numbered list, with an add row
     * at the bottom. It hangs off the same chip as the volume switcher, and the chip
     * (`VolumeBreadcrumb.svelte`) keeps the two from ever being open at once.
     *
     * ❗ The house `Menu` primitive (`#lib/ui/Menu.svelte`) owns every interaction: open
     * and close, anchoring, the cursor, the keyboard (including the digit accelerators
     * and ⌥↑/⌥↓ reorder), keyboard-vs-pointer mode, drag, focus, and placement. This
     * component owns the two keys that swap menus; the rows come from
     * `favorites-menu.svelte.ts`, and a favorite row's label and shortcut field from
     * `FavoriteRowLabel` / `FavoriteShortcutField`, which the switcher shares.
     */
    import { onDestroy } from 'svelte'
    import { getVolumes } from '#lib/stores/volume-store.svelte.js'
    import { getCachedIcon, iconCacheVersion } from '#lib/icon-cache.js'
    import { dependOn } from '#lib/utils/reactivity.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Menu from '#lib/ui/Menu.svelte'
    import { createMenu } from '#lib/ui/menu-controller.svelte.js'
    import type { MenuRowContext } from '#lib/ui/menu-types.js'
    import type { PaneId } from '#lib/commands/types.js'
    import { chooserCommandFor, otherChooserCommandFor } from './header-menu-keys'
    import type { VolumeChangePayload } from '../pane/types'
    import FavoriteRowLabel from './FavoriteRowLabel.svelte'
    import FavoriteShortcutField from './FavoriteShortcutField.svelte'
    import { createFavoritesMenu, FAVORITES_SECTION_ID, type FavoritesRow } from './favorites-menu.svelte'
    import { reportFavoritesMenuOpened, type FavoritesMenuOpenTrigger } from './favorites-analytics'

    interface Props {
        /** Which pane this menu belongs to, so ⌥F1 / ⌥F2 swaps back to ITS switcher. */
        paneId: PaneId
        /** The pane's volume and folder: what the add row acts on, and what gates it. */
        volumeId: string
        currentPath: string
        /** The chip the list hangs under, read at open time. */
        getAnchor: () => HTMLElement | undefined
        /** The chip's whole control cluster: pressing a control in it doesn't close the list. */
        getChipCluster: () => HTMLElement | undefined
        onVolumeChange?: (change: VolumeChangePayload) => void
        /** This pane's chooser key inside the menu: hand the header back to its volume switcher. */
        onShowVolumes: () => void
        /** The OTHER pane's chooser key: hand off to that pane's switcher, like Tab in the switcher. */
        onSwitchPane: () => void
        /** So the chip can keep one header menu open at a time. */
        onOpenChange: (open: boolean) => void
    }

    const {
        paneId,
        volumeId,
        currentPath,
        getAnchor,
        getChipCluster,
        onVolumeChange,
        onShowVolumes,
        onSwitchPane,
        onOpenChange,
    }: Props = $props()

    let renameInputRef: HTMLInputElement | undefined = $state()
    let shortcutInputRef: HTMLInputElement | undefined = $state()

    // Generic macOS folder icon, standing in for a favorite whose own icon isn't fetched
    // yet (FDA-gated paths aren't asked about, to avoid TCC popups). Reading
    // `$iconCacheVersion` re-evaluates this once the icon lands.
    const dirIconFallback = $derived.by(() => {
        dependOn($iconCacheVersion)
        return getCachedIcon('dir')
    })

    const favorites = createFavoritesMenu({
        getVolumes: () => getVolumes(),
        getPaneId: () => paneId,
        getPaneVolumeId: () => volumeId,
        getPaneCurrentPath: () => currentPath,
        getDirIconFallback: () => dirIconFallback,
        getRenameInputRef: () => renameInputRef,
        getShortcutInputRef: () => shortcutInputRef,
        go: (target) => { onVolumeChange?.(target) },
    })

    /** Where focus was when the menu opened, so closing it doesn't move the focused pane. */
    let focusBeforeOpen: HTMLElement | null = null

    /**
     * The keys that swap menus. Central dispatch is suppressed while a header menu is open,
     * so nothing else would answer them; matching through `eventMatchesCommand` means a
     * rebind follows. ⌃D closes (the key that opened it toggles), the pane's OWN chooser
     * key swaps back to its switcher, and the OTHER pane's chooser key hands off to that
     * pane's switcher (⌥F2 over the left pane's favorites means the right pane).
     */
    function handleKey(event: KeyboardEvent): boolean {
        if (eventMatchesCommand(event, 'favorites.open')) {
            menu.close()
            return true
        }
        if (eventMatchesCommand(event, chooserCommandFor(paneId))) {
            onShowVolumes()
            return true
        }
        if (eventMatchesCommand(event, otherChooserCommandFor(paneId))) {
            onSwitchPane()
            return true
        }
        return false
    }

    const menu = createMenu<FavoritesRow>({
        getSections: () => favorites.sections,
        onSelect: (item, source) => {
            void favorites.select(item.data, source)
        },
        onReorder: ({ sectionId, orderedValues }) => {
            if (sectionId === FAVORITES_SECTION_ID) favorites.applyReorder(orderedValues)
        },
        // While either inline editor is focused, its `<input>` owns every keystroke:
        // arrows and Home/End must not move the menu cursor.
        isEditing: favorites.isEditing,
        onKey: handleKey,
        onOpenChange,
        restoreFocus: () => {
            // ❗ Back to whatever held focus, ❌ not to this pane: the command opens the
            // FOCUSED pane's menu, and a swap from the other pane's switcher must not
            // move the focus across.
            focusBeforeOpen?.focus()
            focusBeforeOpen = null
        },
        // ❗ The chip's own controls sit BESIDE the anchor, and pressing one deliberately
        // leaves the menu open.
        keepOpenWithin: getChipCluster,
    })

    export function open(trigger: FavoritesMenuOpenTrigger): void {
        if (menu.isOpen) return
        const anchor = getAnchor()
        if (!anchor) return
        focusBeforeOpen = document.activeElement instanceof HTMLElement ? document.activeElement : null
        menu.openUnder(anchor)
        reportFavoritesMenuOpened({ trigger })
    }

    export function close(): void {
        menu.close()
    }

    export function toggle(trigger: FavoritesMenuOpenTrigger): void {
        if (menu.isOpen) menu.close()
        else open(trigger)
    }

    export function getIsOpen(): boolean {
        return menu.isOpen
    }

    onDestroy(() => {
        menu.destroy()
    })
</script>

<!-- The name this surface carries in Settings > Keyboard shortcuts, so screen readers and
     the shortcut scope say the same thing. -->
<Menu {menu} ariaLabel={tString('shortcuts.scope.favoritesMenu')}>
    {#snippet label(ctx: MenuRowContext<FavoritesRow>)}
        {@const row = ctx.item.data}
        {#if row?.kind === 'favorite'}
            <FavoriteRowLabel {favorites} volume={row.volume} label={ctx.item.label} bind:renameInput={renameInputRef} />
        {:else}
            <span class="row-label">{ctx.item.label}</span>
        {/if}
    {/snippet}
    {#snippet trailing(ctx: MenuRowContext<FavoritesRow>)}
        {@const row = ctx.item.data}
        {#if row?.kind === 'favorite'}
            <FavoriteShortcutField {favorites} volume={row.volume} bind:shortcutInput={shortcutInputRef} />
        {/if}
    {/snippet}
</Menu>

<style>
    .row-label {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
</style>
