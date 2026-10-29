<script lang="ts">
    /**
     * Find and replace inside tag values (not file names), on the fields ticked here: the focused field when the panel opens, else Title to start with, or all.
     * The search follows Multi-Rename's rules (wildcards, `$1` groups with Regular expression, case-insensitive
     * unless Match case); the three option chips are Multi-Rename's own. A value that ends up empty is dropped by the
     * backend. A bad regular expression is worded like Multi-Rename words it.
     *
     * Below the form, saved lists: "Add as step" collects the form's settings into a list in the making (the form
     * empties for the next one), "Save list" keeps it under a name (the form too, if filled), and each saved list runs
     * all its steps in order as one edit (one undo step). The backend seeds the built-ins.
     */
    import { onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { TagField } from '#lib/ipc/bindings.js'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import {
        ReplacePresetRefused,
        deleteTagReplacePreset,
        getTagReplacePresets,
        saveTagReplacePreset,
        type MultiRenameSpec,
        type ReplacePreset,
        type ReplaceStep,
    } from '#lib/tauri-commands/index.js'
    import SearchOptionChips from '../SearchOptionChips.svelte'
    import type { SearchOptionField } from '../search-option-help'
    import { FIELD_LABELS, TAG_FIELDS } from './field-labels'
    import type { TagEditorState } from './tag-editor-state.svelte'
    import type { RunScoped } from './tag-tools'

    interface Props {
        disabled: boolean
        state: TagEditorState
        runScoped: RunScoped
        /** The tag box that had focus when the panel opened; preselected. `null` keeps Title. */
        focusedField?: TagField | null
    }

    const { disabled, state: tags, runScoped, focusedField = null }: Props = $props()

    const CHIPS: readonly SearchOptionField[] = ['caseSensitive', 'firstOnly', 'regex']

    let search = $state('')
    let replacement = $state('')
    let allFields = $state(false)
    // Read once: the panel is mounted when it opens, and later focus changes must not move the ticks.
    // svelte-ignore state_referenced_locally
    let chosen = $state.raw<readonly TagField[]>([focusedField ?? 'title'])
    let options = $state<Pick<MultiRenameSpec, SearchOptionField>>({
        caseSensitive: false,
        firstOnly: false,
        includeExtension: false,
        regex: false,
        substitute: false,
    })
    let badRegex = $state(false)

    const fields = $derived(allFields ? [...TAG_FIELDS] : TAG_FIELDS.filter((field) => chosen.includes(field)))
    const ready = $derived(!disabled && search !== '' && fields.length > 0)

    // The saved lists and the one in the making.
    let presets = $state.raw<readonly ReplacePreset[]>([])
    let steps = $state.raw<readonly ReplaceStep[]>([])
    let listName = $state('')
    let listProblem = $state<string | null>(null)

    const formStep = $derived<ReplaceStep>({
        fields: allFields ? null : fields,
        search,
        replace: replacement,
        caseSensitive: options.caseSensitive,
        firstOnly: options.firstOnly,
        regex: options.regex,
    })
    const stepsToSave = $derived(ready ? [...steps, formStep] : steps)
    const canSave = $derived(!disabled && listName.trim() !== '' && stepsToSave.length > 0)

    onMount(() => {
        getTagReplacePresets().then(
            (loaded) => {
                presets = loaded
            },
            () => {
                listProblem = tString('multiRename.tags.tools.replace.lists.loadFailed')
            },
        )
    })

    function addStep(): void {
        steps = [...steps, formStep]
        search = ''
        replacement = ''
        badRegex = false
    }

    async function saveList(): Promise<void> {
        listProblem = null
        try {
            presets = await saveTagReplacePreset({ id: crypto.randomUUID(), name: listName.trim(), steps: [...stepsToSave] })
            steps = []
            listName = ''
            search = ''
            replacement = ''
        } catch (e) {
            const refusal = e instanceof ReplacePresetRefused ? e.refusal.type : null
            listProblem = tString(
                refusal === 'listFull'
                    ? 'multiRename.tags.tools.replace.lists.full'
                    : refusal === 'empty'
                      ? 'multiRename.tags.tools.replace.lists.needsSteps'
                      : 'multiRename.tags.tools.replace.lists.saveFailed',
            )
        }
    }

    async function deleteList(id: string): Promise<void> {
        presets = await deleteTagReplacePreset(id)
    }

    async function runList(preset: ReplacePreset): Promise<void> {
        badRegex = false
        const ran = await runScoped(async (scope) => {
            await tags.textTool({ kind: 'steps', steps: preset.steps }, scope)
            return true
        })
        badRegex = ran === true && tags.error?.kind === 'badRegex'
    }

    function editList(preset: ReplacePreset): void {
        steps = preset.steps
        listName = preset.name
    }

    function setChosen(field: TagField, on: boolean): void {
        chosen = on ? [...chosen, field] : chosen.filter((f) => f !== field)
    }

    async function run(): Promise<void> {
        badRegex = false
        const ran = await runScoped(async (scope) => {
            await tags.textTool(
                {
                    kind: 'replace',
                    fields,
                    search,
                    replace: replacement,
                    caseSensitive: options.caseSensitive,
                    firstOnly: options.firstOnly,
                    regex: options.regex,
                },
                scope,
            )
            return true
        })
        badRegex = ran === true && tags.error?.kind === 'badRegex'
    }
</script>

<div class="panel">
    <div class="line">
        <label for="tag-replace-search">{tString('multiRename.tags.tools.replace.search')}</label>
        <TextInput id="tag-replace-search" bind:value={search} {disabled} invalid={badRegex} mono oninput={() => { badRegex = false }} />
        <SearchOptionChips
            spec={options}
            only={CHIPS}
            shortcuts={false}
            rendered={null}
            onToggle={(field: SearchOptionField) => { options = { ...options, [field]: !options[field] } }}
        />
    </div>
    <div class="line">
        <label for="tag-replace-with">{tString('multiRename.tags.tools.replace.replaceWith')}</label>
        <TextInput id="tag-replace-with" bind:value={replacement} {disabled} mono />
    </div>
    <div class="fields" role="group" aria-label={tString('multiRename.tags.tools.replace.fieldsLabel')}>
        <Checkbox checked={allFields} {disabled} onCheckedChange={(on: boolean) => { allFields = on }}>
            {tString('multiRename.tags.tools.replace.allFields')}
        </Checkbox>
        {#each TAG_FIELDS as field (field)}
            <Checkbox
                checked={allFields || chosen.includes(field)}
                disabled={disabled || allFields}
                onCheckedChange={(on: boolean) => { setChosen(field, on) }}
            >
                {tString(FIELD_LABELS[field])}
            </Checkbox>
        {/each}
    </div>
    {#if badRegex}
        <p class="error" role="alert">{tString('multiRename.error.badRegex')}</p>
    {/if}
    <div class="line">
        <Button size="mini" variant="primary" disabled={!ready} onclick={() => void run()}>
            {tString('multiRename.tags.tools.replace.run')}
        </Button>
    </div>
    <div class="lists" role="group" aria-label={tString('multiRename.tags.tools.replace.lists.heading')}>
        <h3>{tString('multiRename.tags.tools.replace.lists.heading')}</h3>
        {#if steps.length > 0}
            <ol class="steps" aria-label={tString('multiRename.tags.tools.replace.lists.stepsLabel')}>
                {#each steps as item, at (at)}
                    <li>
                        <span class="mono">
                            {tString('multiRename.tags.tools.replace.lists.stepSummary', { search: item.search, replace: item.replace })}
                        </span>
                        <Button
                            size="mini"
                            {disabled}
                            onclick={() => { steps = steps.filter((_, i) => i !== at) }}
                        >
                            {tString('multiRename.tags.tools.replace.lists.removeStep')}
                        </Button>
                    </li>
                {/each}
            </ol>
        {/if}
        <div class="line">
            <Button size="mini" disabled={!ready} onclick={addStep}>
                {tString('multiRename.tags.tools.replace.lists.addStep')}
            </Button>
            <label for="tag-replace-list-name">{tString('multiRename.tags.tools.replace.lists.name')}</label>
            <TextInput id="tag-replace-list-name" bind:value={listName} {disabled} />
            <Button size="mini" disabled={!canSave} onclick={() => void saveList()}>
                {tString('multiRename.tags.tools.replace.lists.save')}
            </Button>
        </div>
        {#if listProblem !== null}
            <p class="note">{listProblem}</p>
        {/if}
        {#if presets.length === 0}
            <p class="note">{tString('multiRename.tags.tools.replace.lists.empty')}</p>
        {:else}
            <ul class="saved">
                {#each presets as preset (preset.id)}
                    <li>
                        <span class="name">{preset.name}</span>
                        <Button
                            size="mini"
                            variant="primary"
                            {disabled}
                            aria-label={tString('multiRename.tags.tools.replace.lists.runNamed', { name: preset.name })}
                            onclick={() => void runList(preset)}
                        >
                            {tString('multiRename.tags.tools.replace.lists.run')}
                        </Button>
                        <Button
                            size="mini"
                            {disabled}
                            aria-label={tString('multiRename.tags.tools.replace.lists.editNamed', { name: preset.name })}
                            onclick={() => { editList(preset) }}
                        >
                            {tString('multiRename.tags.tools.replace.lists.edit')}
                        </Button>
                        <Button
                            size="mini"
                            {disabled}
                            aria-label={tString('multiRename.tags.tools.replace.lists.deleteNamed', { name: preset.name })}
                            onclick={() => void deleteList(preset.id)}
                        >
                            {tString('multiRename.tags.tools.replace.lists.delete')}
                        </Button>
                    </li>
                {/each}
            </ul>
        {/if}
    </div>
</div>

<style>
    .panel {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .line {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xs);
    }

    label {
        min-width: 80px;
        color: var(--color-text-secondary);
    }

    .fields {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs) var(--spacing-md);
    }

    .lists {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        border-top: 1px solid var(--color-border-subtle);
        padding-top: var(--spacing-xs);
    }

    h3 {
        margin: 0;
        font-size: inherit;
        font-weight: 600;
    }

    .steps,
    .saved {
        margin: 0;
        padding: 0;
        list-style: none;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .steps li,
    .saved li {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .name {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .mono {
        font-family: var(--font-mono);
    }

    .note {
        margin: 0;
        color: var(--color-text-secondary);
    }

    .error {
        margin: 0;
        color: var(--color-error-text);
    }
</style>
