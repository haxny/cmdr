<script lang="ts">
    /**
     * The Tags tab's clean-up tools, one button each; every tool runs in the backend over all the session's files.
     * Case changes the focused field, or with none the five name fields, and says which. Renumber is hidden for a
     * single file (a single-file book has no track number). Auto-number… opens the numbering wizard (always, since it
     * also numbers discs). Remove extras… lists the files' extra tags with nothing
     * checked, so nothing goes without an explicit choice. A second row opens the text tools' panels (Find and replace,
     * Format a field, From file name), one at a time; they call their tools through `runScoped`, which the tab gives the
     * checked tracks.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { formatUnitList } from '#lib/intl/list-format.js'
    import type { CaseTool, TagField, TagTool } from '#lib/ipc/bindings.js'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import { FIELD_LABELS } from './field-labels'
    import FormatPanel from './FormatPanel.svelte'
    import NamePatternPanel from './NamePatternPanel.svelte'
    import NumberingDialog from './NumberingDialog.svelte'
    import ReplacePanel from './ReplacePanel.svelte'
    import type { TagEditorState } from './tag-editor-state.svelte'
    import { DIACRITICS_FIELDS, caseFields, caseTool, removeExtrasTool, showsRenumber, type RunScoped } from './tag-tools'

    interface Props {
        trackCount: number
        /** The files' extra (unmanaged) tag keys. */
        extras: string[]
        /** The field whose box last had focus, which Case changes; `null` for none. */
        focusedField: TagField | null
        disabled: boolean
        onTool: (tool: TagTool) => void
        /** The tab's state and scoped runner, for the text tools' panels. */
        state: TagEditorState
        runScoped: RunScoped
        scopeKey: string
    }

    const { trackCount, extras, focusedField, disabled, onTool, state: tags, runScoped, scopeKey }: Props = $props()

    type PanelId = 'case' | 'extras' | 'replace' | 'format' | 'name'

    /** The text tools: a button each, opening its panel under the bar. */
    const TEXT_TOOLS: readonly { id: 'replace' | 'format' | 'name'; label: MessageKey }[] = [
        { id: 'replace', label: 'multiRename.tags.tools.replace.button' },
        { id: 'format', label: 'multiRename.tags.tools.format.button' },
        { id: 'name', label: 'multiRename.tags.tools.name.button' },
    ]

    const CASES: readonly { value: CaseTool; label: MessageKey }[] = [
        { value: 'sentence', label: 'multiRename.tags.tools.caseSentence' },
        { value: 'title', label: 'multiRename.tags.tools.caseTitle' },
        { value: 'lower', label: 'multiRename.tags.tools.caseLower' },
        { value: 'upper', label: 'multiRename.tags.tools.caseUpper' },
    ]

    /** The plain tools: one press, one tool. */
    const PLAIN: readonly { label: MessageKey; tool: TagTool }[] = [
        { label: 'multiRename.tags.tools.stripTrackPrefix', tool: { kind: 'stripTrackPrefix' } },
        { label: 'multiRename.tags.tools.albumArtistFromArtist', tool: { kind: 'albumArtistFromArtist' } },
        { label: 'multiRename.tags.tools.narratorFromText', tool: { kind: 'narratorFromText' } },
        { label: 'multiRename.tags.tools.clearRedundantTitles', tool: { kind: 'clearRedundantTitles' } },
    ]

    let open = $state<PanelId | null>(null)
    let numbering = $state(false)
    let checked = $state.raw<readonly string[]>([])

    const caseFieldNames = $derived(formatUnitList(caseFields(focusedField).map((field) => tString(FIELD_LABELS[field]))))

    function toggle(panel: PanelId): void {
        open = open === panel ? null : panel
        checked = []
    }

    function setChecked(key: string, on: boolean): void {
        checked = on ? [...checked, key] : checked.filter((k) => k !== key)
    }

    function run(tool: TagTool | null): void {
        if (tool === null) return
        open = null
        checked = []
        onTool(tool)
    }
</script>

<div class="tools" role="group" aria-label={tString('multiRename.tags.tools.label')}>
    <div class="row">
        <Button
            size="mini"
            {disabled}
            aria-expanded={open === 'case'}
            aria-controls="tag-tools-case"
            onclick={() => { toggle('case') }}
        >
            {tString('multiRename.tags.tools.caseButton')}
        </Button>
        <Button
            size="mini"
            {disabled}
            tooltipContent={{ text: tString('multiRename.tags.tools.removeDiacriticsTooltip') }}
            onclick={() => { run({ kind: 'stripDiacritics', fields: [...DIACRITICS_FIELDS] }) }}
        >
            {tString('multiRename.tags.tools.removeDiacritics')}
        </Button>
        {#if showsRenumber(trackCount)}
            <Button size="mini" {disabled} onclick={() => { run({ kind: 'renumber', start: 1 }) }}>
                {tString('multiRename.tags.tools.renumber')}
            </Button>
        {/if}
        <Button size="mini" {disabled} onclick={() => { numbering = true }}>
            {tString('multiRename.tags.numbering.button')}
        </Button>
        {#each PLAIN as plain (plain.label)}
            <Button size="mini" {disabled} onclick={() => { run(plain.tool) }}>{tString(plain.label)}</Button>
        {/each}
        <Button
            size="mini"
            disabled={disabled || extras.length === 0}
            aria-expanded={open === 'extras'}
            aria-controls="tag-tools-extras"
            onclick={() => { toggle('extras') }}
        >
            {tString('multiRename.tags.tools.removeExtras')}
        </Button>
    </div>
    <div class="row" role="group" aria-label={tString('multiRename.tags.tools.moreLabel')}>
        {#each TEXT_TOOLS as text (text.id)}
            <Button
                size="mini"
                {disabled}
                aria-expanded={open === text.id}
                aria-controls="tag-tools-{text.id}"
                onclick={() => { toggle(text.id) }}
            >
                {tString(text.label)}
            </Button>
        {/each}
    </div>
    {#if open === 'case' && !disabled}
        <div class="panel" id="tag-tools-case">
            {#each CASES as choice (choice.value)}
                <Button size="mini" onclick={() => { run(caseTool(choice.value, focusedField)) }}>{tString(choice.label)}</Button>
            {/each}
            <span class="hint">{tString('multiRename.tags.tools.caseFields', { fields: caseFieldNames })}</span>
        </div>
    {:else if open === 'extras' && !disabled}
        <div class="panel extras" id="tag-tools-extras">
            <ul aria-label={tString('multiRename.tags.tools.extrasLabel')}>
                {#each extras as key (key)}
                    <li>
                        <Checkbox checked={checked.includes(key)} onCheckedChange={(on: boolean) => { setChecked(key, on) }}>{key}</Checkbox>
                    </li>
                {/each}
            </ul>
            <Button size="mini" disabled={checked.length === 0} onclick={() => { run(removeExtrasTool(checked)) }}>
                {tString('multiRename.tags.tools.removeChecked', { count: checked.length, countText: formatInteger(checked.length) })}
            </Button>
        </div>
    {:else if open === 'replace' && !disabled}
        <div class="panel text-panel" id="tag-tools-replace"><ReplacePanel {disabled} state={tags} {runScoped} /></div>
    {:else if open === 'format' && !disabled}
        <div class="panel text-panel" id="tag-tools-format"><FormatPanel {disabled} state={tags} {runScoped} /></div>
    {:else if open === 'name' && !disabled}
        <div class="panel text-panel" id="tag-tools-name"><NamePatternPanel {disabled} state={tags} {runScoped} {scopeKey} /></div>
    {/if}
</div>

{#if numbering}
    <NumberingDialog {trackCount} state={tags} {runScoped} onClose={() => { numbering = false }} />
{/if}

<style>
    .tools {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .row,
    .panel {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .hint {
        color: var(--color-text-secondary);
    }

    .text-panel {
        display: block;
    }

    .extras {
        flex-direction: column;
        align-items: flex-start;
    }

    ul {
        margin: 0;
        padding: 0;
        list-style: none;
        max-height: 140px;
        overflow: auto;
    }
</style>
