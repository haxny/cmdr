<script lang="ts">
    /**
     * The Synchronize folders footer's `[Profiles ▾ <name>]` (spec 4.6), the multi-rename `PresetsControl` pattern:
     * one button (F2) opening the menu of saved profiles (numbered 1–9, each with Rename / Move up / Move down /
     * Delete) and Save as profile (⌘S), plus the name popover that asks before replacing a taken name. The dialog
     * owns the keys and calls `toggleMenu` / `openSave`; loading hands the profile to `onLoad`.
     *
     * Never silent: a full list or a profile that fails the checks keeps the popover open with why; a file that
     * couldn't be written, or anything untyped, is a toast.
     */
    import { onDestroy, onMount } from 'svelte'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import Menu from '#lib/ui/Menu.svelte'
    import Popover from '#lib/ui/Popover.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import { createMenu } from '#lib/ui/menu-controller.svelte.js'
    import type { MenuItem } from '#lib/ui/menu-types.js'
    import { addToast } from '#lib/ui/toast/index.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import {
        profileView,
        type LoadedProfile,
        type ProfileAnswer,
        type ProfileRefusal,
        type ProfileSource,
        type SyncProfiles,
    } from './profiles.svelte'
    import { profileMenuSections, profileProblemText, type ProfileAction } from './sync-profile-menu'

    interface Props {
        profiles: SyncProfiles
        /** The id of the profile the dialog runs as. */
        loadedId: string | null
        /** What a save keeps: the dialog's roots, options, show-filters, and overrides now. */
        source: () => ProfileSource
        onLoad: (profile: LoadedProfile) => void
    }

    const { profiles, loadedId, source, onLoad }: Props = $props()

    type NameMode = { kind: 'save' } | { kind: 'rename'; id: string }

    let wrapperEl = $state<HTMLSpanElement>()
    let buttonEl = $state<HTMLButtonElement>()
    let nameInput = $state<HTMLInputElement>()
    let nameMode = $state<NameMode | null>(null)
    let name = $state('')
    let restoreFilters = $state(false)
    /** The name the person was told is taken; pressing again replaces. Editing the name forgets it. */
    let clash = $state<string | null>(null)
    /** Why the last save didn't land, said in the popover. */
    let refusalText = $state<string | null>(null)

    const views = $derived(profiles.entries.map(profileView))
    const loadedName = $derived(views.find((view) => view.id === loadedId)?.name ?? null)

    const menu = createMenu<ProfileAction>({
        getSections: () => profileMenuSections(views, loadedId),
        onSelect: (item: MenuItem<ProfileAction>) => {
            if (item.data) act(item.data)
        },
        restoreFocus: () => {
            buttonEl?.focus()
        },
    })

    onMount(() => {
        buttonEl = wrapperEl?.querySelector('button') ?? undefined
        void profiles.refresh()
    })

    onDestroy(() => {
        menu.destroy()
    })

    function act(action: ProfileAction): void {
        switch (action.kind) {
            case 'load': {
                const loaded = profiles.load(action.id)
                if (loaded) onLoad(loaded)
                return
            }
            case 'invalid':
                addToast(profileProblemText(action.problem), { level: 'info' })
                return
            case 'saveAs':
                openSave()
                return
            case 'rename':
                openName({ kind: 'rename', id: action.id }, views.find((view) => view.id === action.id)?.name ?? '')
                return
            case 'move':
                void profiles.move(action.id, action.direction).then(toastIfRefused)
                return
            case 'delete':
                void profiles.remove(action.id).then(toastIfRefused)
                return
        }
    }

    function refusalWords(refusal: ProfileRefusal): string {
        switch (refusal.type) {
            case 'listFull':
                return tString('sync.profiles.listFull', { maxText: formatNumber(refusal.max) })
            case 'invalid':
                return tString('sync.profiles.invalid', { problem: profileProblemText(refusal.problem) })
            case 'writeFailed':
                return tString('sync.profiles.writeFailed')
            case 'nameTaken':
            case 'unexpected':
                return tString('sync.profiles.unexpected')
        }
    }

    function toastIfRefused(answer: ProfileAnswer): void {
        if (answer.type === 'refused') addToast(refusalWords(answer.refusal), { level: 'warn' })
    }

    /** F2: opens or closes the menu under its button. */
    export function toggleMenu(): void {
        if (nameMode !== null || !buttonEl) return
        menu.toggleUnder(buttonEl)
    }

    /** ⌘S: the name popover, prefilled with the loaded profile's name. */
    export function openSave(): void {
        menu.close()
        openName({ kind: 'save' }, loadedName ?? '')
    }

    function openName(mode: NameMode, prefill: string): void {
        if (nameMode !== null) return
        name = prefill
        clash = null
        refusalText = null
        restoreFilters = false
        nameMode = mode
        queueMicrotask(() => {
            requestAnimationFrame(() => nameInput?.select())
        })
    }

    function closeName(): void {
        nameMode = null
        buttonEl?.focus()
    }

    async function submitName(): Promise<void> {
        const mode = nameMode
        if (!mode || name.trim() === '') return
        const replace = clash !== null
        const answer =
            mode.kind === 'save'
                ? await profiles.saveAs(name, source(), { replace, restoreShowFilters: restoreFilters })
                : await profiles.rename(mode.id, name, { replace })
        if (answer.type === 'saved') {
            closeName()
            return
        }
        if (answer.type === 'nameTaken') {
            clash = answer.name
            return
        }
        if (answer.refusal.type === 'nameTaken') {
            clash = name.trim()
            return
        }
        if (answer.refusal.type === 'listFull' || answer.refusal.type === 'invalid') {
            refusalText = refusalWords(answer.refusal)
            return
        }
        closeName()
        toastIfRefused(answer)
    }

    function handleNameKeydown(e: KeyboardEvent): void {
        if (e.key !== 'Enter' || e.isComposing) return
        claimKey(e)
        void submitName()
    }
</script>

<span class="profiles" bind:this={wrapperEl}>
    <Button
        onclick={toggleMenu}
        aria-haspopup="menu"
        tooltipContent={tString('sync.profiles.tooltip')}
        keyHint="F2"
    >
        <span class="label">
            {tString('sync.profiles.title')}
            <Icon name="chevron-down" size={14} aria-hidden="true" />
            {#if loadedName !== null}<span class="loaded">{loadedName}</span>{/if}
        </span>
    </Button>
</span>

<Menu {menu} ariaLabel={tString('sync.profiles.title')} minWidth={260} />

{#if buttonEl}
    <Popover
        anchor={buttonEl}
        open={nameMode !== null}
        onClose={closeName}
        ariaLabel={nameMode?.kind === 'rename' ? tString('sync.profiles.renameTitle') : tString('sync.profiles.saveTitle')}
    >
        <div class="name-form">
            <label class="field">
                <span class="field-label">{tString('sync.profiles.name')}</span>
                <TextInput
                    bind:inputElement={nameInput}
                    value={name}
                    oninput={(e: Event) => {
                        name = (e.currentTarget as HTMLInputElement).value
                        clash = null
                        refusalText = null
                    }}
                    onkeydown={handleNameKeydown}
                    ariaLabel={tString('sync.profiles.name')}
                    warning={clash !== null || refusalText !== null}
                />
            </label>
            {#if nameMode?.kind === 'save'}
                <Checkbox bind:checked={restoreFilters}>{tString('sync.profiles.restoreFilters')}</Checkbox>
            {/if}
            {#if clash !== null}
                <p class="note" role="alert">{tString('sync.profiles.clash', { name: clash })}</p>
            {:else if refusalText !== null}
                <p class="note" role="alert">{refusalText}</p>
            {/if}
            <div class="actions">
                <Button variant="primary" size="mini" disabled={name.trim() === ''} onclick={() => void submitName()}>
                    {#if clash !== null}
                        {tString('sync.profiles.replace')}
                    {:else if nameMode?.kind === 'rename'}
                        {tString('sync.profiles.renameConfirm')}
                    {:else}
                        {tString('sync.profiles.save')}
                    {/if}
                </Button>
            </div>
        </div>
    </Popover>
{/if}

<style>
    .profiles {
        display: inline-flex;
    }

    .label {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
        max-width: 280px;
    }

    .loaded {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-secondary);
    }

    .name-form {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 280px;
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .field-label,
    .note {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
    }
</style>
