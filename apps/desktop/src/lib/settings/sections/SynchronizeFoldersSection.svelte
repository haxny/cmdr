<script lang="ts">
    /**
     * Settings > Behavior > Synchronize folders.
     *
     * Three cards: where files Synchronize replaces or removes are kept (policy, versions
     * folder location, name, custom folder), how modified times are compared, and the
     * limits that make Mirror ask before removing. Every row is a registry setting rendered
     * through the house primitives; the dialog reads them at compare and at run and hands
     * them to the backend as `SyncDefaults`, so nothing here applies anything.
     *
     * The versions folder fields are checked by the backend's own rules
     * (`checkSyncKeepPolicy`) and a problem is shown under the field, never fixed silently.
     * Whether a custom folder lies inside a synchronized folder is only known once a run
     * has roots; the confirm step shows that one.
     *
     * Card visibility is section-owned (`anyVisible(shouldShow, ...)`) over the same
     * `shouldShow` that gates the search.
     */
    import SettingsSection from '../components/SettingsSection.svelte'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SettingTextInput from '../components/SettingTextInput.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { getSetting, getSettingDefinition, onSpecificSettingChange } from '#lib/settings/index.js'
    import { createShouldShow, anyVisible } from '#lib/settings/settings-search.js'
    import { disabledNoteId } from '#lib/settings/settings-window.js'
    import { keepPolicyFromSettings } from '#lib/settings/sync-keep-policy.js'
    import { checkSyncKeepPolicy, type KeepRefusal } from '#lib/tauri-commands/index.js'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const POLICY = 'sync.keep.policy'
    const LOCATION = 'sync.keep.location'
    const FOLDER_NAME = 'sync.keep.folderName'
    const CUSTOM_FOLDER = 'sync.keep.customFolder'
    const TOLERANCE = 'sync.compare.timeTolerance'
    const HOUR_SHIFT = 'sync.compare.ignoreHourShift'
    const MAX_PERCENT = 'sync.guards.maxRemovePercent'
    const MAX_COUNT = 'sync.guards.maxRemoveCount'

    const emptyDef = { label: '', description: '' }
    const policyDef = getSettingDefinition(POLICY) ?? emptyDef
    const locationDef = getSettingDefinition(LOCATION) ?? emptyDef
    const folderNameDef = getSettingDefinition(FOLDER_NAME) ?? emptyDef
    const customFolderDef = getSettingDefinition(CUSTOM_FOLDER) ?? emptyDef
    const toleranceDef = getSettingDefinition(TOLERANCE) ?? emptyDef
    const hourShiftDef = getSettingDefinition(HOUR_SHIFT) ?? emptyDef
    const maxPercentDef = getSettingDefinition(MAX_PERCENT) ?? emptyDef
    const maxCountDef = getSettingDefinition(MAX_COUNT) ?? emptyDef

    let policy = $state(getSetting(POLICY))
    let location = $state(getSetting(LOCATION))
    let folderName = $state(getSetting(FOLDER_NAME))
    let customFolder = $state(getSetting(CUSTOM_FOLDER))
    $effect(() =>
        onSpecificSettingChange(POLICY, (v) => {
            policy = v
        }),
    )
    $effect(() =>
        onSpecificSettingChange(LOCATION, (v) => {
            location = v
        }),
    )
    $effect(() =>
        onSpecificSettingChange(FOLDER_NAME, (v) => {
            folderName = v
        }),
    )
    $effect(() =>
        onSpecificSettingChange(CUSTOM_FOLDER, (v) => {
            customFolder = v
        }),
    )

    const customLocation = $derived(location === 'customFolder')

    // What the backend says about the versions folder these settings name. A newer answer
    // replaces an older one; one that arrives for settings that have since changed is dropped.
    let refusal = $state<KeepRefusal | null>(null)
    $effect(() => {
        const settings = { policy, location, folderName, customFolder }
        let current = true
        void checkSyncKeepPolicy(keepPolicyFromSettings(settings)).then(
            (answer) => {
                if (current) refusal = answer
            },
            () => {
                if (current) refusal = null
            },
        )
        return () => {
            current = false
        }
    })

    const problem = $derived.by(() => {
        if (refusal === 'escapesFolder') return tString('settings.sync.keep.problem.escapes')
        if (refusal !== 'badTemplate') return null
        return customLocation
            ? tString('settings.sync.keep.problem.customFolder', PLACEHOLDER_WORDS)
            : tString('settings.sync.keep.problem.targetFolder')
    })
    const PROBLEM_ID = 'sync-keep-problem'
    // The words the custom folder's path may carry; the messages take them as parameters because their
    // braces are the path syntax, not message syntax.
    const PLACEHOLDER_WORDS = { root: '{root}', profile: '{profile}', date: '{date}' }
</script>

{#snippet customFolderDescription()}
    {customFolderDef.description}
    {tString('settings.sync.keep.customFolder.placeholders', PLACEHOLDER_WORDS)}
{/snippet}

<SettingsSection title={tString('settings.section.synchronizeFolders')}>
    {#if anyVisible(shouldShow, POLICY, LOCATION, FOLDER_NAME, CUSTOM_FOLDER)}
        <SectionCard label={tString('settings.sync.card.keep')}>
            {#if shouldShow(POLICY)}
                <SettingRow id={POLICY} label={policyDef.label} description={policyDef.description} {searchQuery}>
                    <SettingRadioGroup id={POLICY} />
                </SettingRow>
            {/if}
            {#if shouldShow(LOCATION)}
                <SettingRow id={LOCATION} label={locationDef.label} description={locationDef.description} {searchQuery}>
                    <SettingToggleGroup id={LOCATION} />
                </SettingRow>
            {/if}
            {#if shouldShow(FOLDER_NAME)}
                <SettingRow
                    id={FOLDER_NAME}
                    label={folderNameDef.label}
                    description={folderNameDef.description}
                    disabled={customLocation}
                    disabledNote={tString('settings.sync.keep.folderName.disabledNote')}
                    {searchQuery}
                >
                    <SettingTextInput
                        id={FOLDER_NAME}
                        disabled={customLocation}
                        aria-describedby={customLocation
                            ? disabledNoteId(FOLDER_NAME)
                            : problem
                              ? PROBLEM_ID
                              : undefined}
                        invalid={!customLocation && problem !== null}
                    />
                </SettingRow>
            {/if}
            {#if shouldShow(CUSTOM_FOLDER)}
                <SettingRow
                    id={CUSTOM_FOLDER}
                    label={customFolderDef.label}
                    description={customFolderDef.description}
                    disabled={!customLocation}
                    disabledNote={tString('settings.sync.keep.customFolder.disabledNote')}
                    descriptionContent={customFolderDescription}
                    {searchQuery}
                >
                    <SettingTextInput
                        id={CUSTOM_FOLDER}
                        disabled={!customLocation}
                        aria-describedby={!customLocation
                            ? disabledNoteId(CUSTOM_FOLDER)
                            : problem
                              ? PROBLEM_ID
                              : undefined}
                        invalid={customLocation && problem !== null}
                    />
                </SettingRow>
            {/if}
            {#if problem}
                <p class="problem" id={PROBLEM_ID} role="alert">{problem}</p>
            {/if}
        </SectionCard>
    {/if}

    {#if anyVisible(shouldShow, TOLERANCE, HOUR_SHIFT)}
        <SectionCard label={tString('settings.sync.card.compare')}>
            {#if shouldShow(TOLERANCE)}
                <SettingRow id={TOLERANCE} label={toleranceDef.label} description={toleranceDef.description} {searchQuery}>
                    <SettingNumberInput id={TOLERANCE} unit={tString('settings.sync.compare.timeTolerance.unit')} />
                </SettingRow>
            {/if}
            {#if shouldShow(HOUR_SHIFT)}
                <SettingRow id={HOUR_SHIFT} label={hourShiftDef.label} description={hourShiftDef.description} {searchQuery}>
                    <SettingSwitch id={HOUR_SHIFT} />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}

    {#if anyVisible(shouldShow, MAX_PERCENT, MAX_COUNT)}
        <SectionCard label={tString('settings.sync.card.guards')}>
            {#if shouldShow(MAX_PERCENT)}
                <SettingRow
                    id={MAX_PERCENT}
                    label={maxPercentDef.label}
                    description={maxPercentDef.description}
                    {searchQuery}
                >
                    <SettingNumberInput id={MAX_PERCENT} unit={tString('settings.sync.guards.maxRemovePercent.unit')} />
                </SettingRow>
            {/if}
            {#if shouldShow(MAX_COUNT)}
                <SettingRow id={MAX_COUNT} label={maxCountDef.label} description={maxCountDef.description} {searchQuery}>
                    <SettingNumberInput id={MAX_COUNT} />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .problem {
        margin: var(--spacing-xs) 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-error-text);
    }
</style>
