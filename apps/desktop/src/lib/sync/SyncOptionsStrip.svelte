<script lang="ts">
    /**
     * The options strip: the two folders with Swap (⌘U), the mode, what compares, the toggles, and the masks. A
     * change here never compares on its own; the session marks the grid stale and the dialog shows "Compare again"
     * (ForkLift's re-scan on every change was the complaint). Enter in a field compares (the dialog's key map).
     */
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import ToggleGroup from '#lib/ui/ToggleGroup.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { CompareBy, Mode, SyncOptions } from '#lib/tauri-commands/sync.js'
    import { excludesFromText, excludesToText } from './sync-options'
    import type { SyncSession } from './sync-session.svelte'

    interface Props {
        session: SyncSession
        onSwap: () => void
    }

    const { session, onSwap }: Props = $props()

    const options = $derived(session.options)

    const MODE_TEXT: Record<Mode, { label: MessageKey; hint: MessageKey }> = {
        compareOnly: { label: 'sync.options.mode.compareOnly', hint: 'sync.options.modeHint.compareOnly' },
        bothWays: { label: 'sync.options.mode.bothWays', hint: 'sync.options.modeHint.bothWays' },
        update: { label: 'sync.options.mode.update', hint: 'sync.options.modeHint.update' },
        mirror: { label: 'sync.options.mode.mirror', hint: 'sync.options.modeHint.mirror' },
    }
    const MODES = Object.keys(MODE_TEXT) as Mode[]
    const modeOptions = $derived(
        MODES.map((mode) => ({ value: mode, label: tString(MODE_TEXT[mode].label), tooltip: tString(MODE_TEXT[mode].hint) })),
    )
    const COMPARE_BY_TEXT: Record<CompareBy, MessageKey> = {
        dateAndSize: 'sync.options.compareBy.dateAndSize',
        sizeOnly: 'sync.options.compareBy.sizeOnly',
        content: 'sync.options.compareBy.content',
    }
    const COMPARE_BY = Object.keys(COMPARE_BY_TEXT) as CompareBy[]
    const compareByOptions = $derived(COMPARE_BY.map((value) => ({ value, label: tString(COMPARE_BY_TEXT[value]) })))

    type Toggle = 'subfolders' | 'onlySelected' | 'ignoreDate' | 'emptyFolders'
    const TOGGLE_TEXT: Record<Toggle, MessageKey> = {
        subfolders: 'sync.options.subfolders',
        onlySelected: 'sync.options.onlySelected',
        ignoreDate: 'sync.options.ignoreDate',
        emptyFolders: 'sync.options.emptyFolders',
    }
    const TOGGLES = Object.keys(TOGGLE_TEXT) as Toggle[]

    function set<K extends keyof SyncOptions>(key: K, value: SyncOptions[K]): void {
        session.setOption(key, value)
    }

    const isMode = (value: string): value is Mode => (MODES as string[]).includes(value)
    const isCompareBy = (value: string): value is CompareBy => (COMPARE_BY as string[]).includes(value)
</script>

<div class="strip" role="group" aria-label={tString('sync.dialog.optionsLabel')}>
    <div class="roots">
        <span class="root-label">{tString('sync.options.left')}</span>
        <span class="root-path" data-side="left">{session.roots.left.root.path}</span>
        <Button size="mini" variant="secondary" keyHint="⌘U" aria-keyshortcuts="Meta+U" onclick={onSwap}
            >{tString('sync.options.swap')}</Button
        >
        <span class="root-label">{tString('sync.options.right')}</span>
        <span class="root-path" data-side="right">{session.roots.right.root.path}</span>
    </div>
    <div class="choices">
        <span class="choice-label">{tString('sync.options.mode')}</span>
        <ToggleGroup
            semantics="toggles"
            ariaLabel={tString('sync.options.mode')}
            value={options.mode}
            options={modeOptions}
            onChange={(value: string) => {
                if (isMode(value)) set('mode', value)
            }}
        />
        <span class="choice-label">{tString('sync.options.compareBy')}</span>
        <ToggleGroup
            semantics="toggles"
            ariaLabel={tString('sync.options.compareBy')}
            value={options.compareBy}
            options={compareByOptions}
            onChange={(value: string) => {
                if (isCompareBy(value)) set('compareBy', value)
            }}
        />
    </div>
    <div class="toggles">
        {#each TOGGLES as toggle (toggle)}
            <Checkbox
                checked={options[toggle]}
                onCheckedChange={(checked: boolean) => {
                    set(toggle, checked)
                }}>{tString(TOGGLE_TEXT[toggle])}</Checkbox
            >
        {/each}
    </div>
    <div class="masks">
        <label class="mask-field">
            <span>{tString('sync.options.masks')}</span>
            <TextInput
                mono
                value={options.masks}
                placeholder={tString('sync.options.masksPlaceholder')}
                aria-describedby="sync-masks-hint"
                oninput={(event: Event & { currentTarget: HTMLInputElement }) => {
                    set('masks', event.currentTarget.value)
                }}
            />
        </label>
        <label class="mask-field">
            <span>{tString('sync.options.excludes')}</span>
            <TextInput
                mono
                value={excludesToText(options.excludes)}
                aria-describedby="sync-excludes-hint"
                oninput={(event: Event & { currentTarget: HTMLInputElement }) => {
                    set('excludes', excludesFromText(event.currentTarget.value))
                }}
            />
        </label>
    </div>
    <span class="sr-only" id="sync-masks-hint">{tString('sync.options.masksHint')}</span>
    <span class="sr-only" id="sync-excludes-hint">{tString('sync.options.excludesHint')}</span>
</div>

<style>
    .strip {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .roots,
    .choices,
    .toggles,
    .masks {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .root-label,
    .choice-label {
        color: var(--color-text-secondary);
    }

    .root-path {
        overflow: hidden;
        min-width: 0;
        flex: 1 1 0;
        font-family: var(--font-mono);
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .mask-field {
        display: flex;
        flex: 1 1 200px;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
