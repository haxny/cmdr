<script lang="ts">
    /**
     * Find and replace inside tag values (not file names), on the fields ticked here: the focused field when the panel opens, else Title to start with, or all.
     * The search follows Multi-Rename's rules (wildcards, `$1` groups with Regular expression, case-insensitive
     * unless Match case); the three option chips are Multi-Rename's own. A value that ends up empty is dropped by the
     * backend. A bad regular expression is worded like Multi-Rename words it.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { TagField } from '#lib/ipc/bindings.js'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import type { MultiRenameSpec } from '#lib/tauri-commands/index.js'
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

    .error {
        margin: 0;
        color: var(--color-error-text);
    }
</style>
