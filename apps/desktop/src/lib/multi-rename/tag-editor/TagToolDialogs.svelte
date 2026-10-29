<script lang="ts">
    /**
     * The tag tools, each in a dialog over the Tags window's panes (never in the flow, so the panes never move). The
     * function-key bar or the tool's key opens one at a time (`open`). The text tools (Find and replace, Format a
     * field, Tags from file names) are their Plan 2b panels, which stay open to run again until closed; the
     * numbering wizard is its own dialog. Case, Clean up, Remove extras and Clear run once and close. Every tool goes
     * through the window's `runScoped`, so it works on the Tracks pane's selection (all when none is selected, nothing
     * when the selected tracks are gone). Case and Clear start on the field the user was on when the tool opened
     * (`field`), or with none the five name fields (Case) and Title (Clear). Remove extras starts with nothing ticked.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { formatUnitList } from '#lib/intl/list-format.js'
    import type { CaseTool, TagField, TagTool } from '#lib/ipc/bindings.js'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Select from '#lib/ui/Select.svelte'
    import { FIELD_LABELS, TAG_FIELDS } from './field-labels'
    import FormatPanel from './FormatPanel.svelte'
    import NamePatternPanel from './NamePatternPanel.svelte'
    import NumberingDialog from './NumberingDialog.svelte'
    import ReplacePanel from './ReplacePanel.svelte'
    import type { TagEditorState } from './tag-editor-state.svelte'
    import {
        DIACRITICS_FIELDS,
        caseFields,
        caseTool,
        removeExtrasTool,
        showsRenumber,
        textTool,
        type RunScoped,
        type TextOptions,
        type TagToolId,
    } from './tag-tools'

    interface Props {
        open: TagToolId
        /** The field the user was on when the tool opened (the Album cursor row, or Title in Tracks); `null` for none. */
        field: TagField | null
        trackCount: number
        /** How many tracks are selected (0: the tools work on all). */
        selectedCount: number
        /** With nothing selected and the Tracks filter on: how many tracks it shows (the tools' scope); else `null`. */
        shownCount?: number | null
        /** The files' extra (unmanaged) tag keys. */
        extras: string[]
        state: TagEditorState
        runScoped: RunScoped
        scopeKey: string
        /** Runs a one-press tool on the selection. */
        onTool: (tool: TagTool) => void
        /** Empties `field` on the selection. */
        onClear: (field: TagField) => void
        onClose: () => void
    }

    const { open, field, trackCount, selectedCount, shownCount = null, extras, state: tags, runScoped, scopeKey, onTool, onClear, onClose }: Props =
        $props()

    const TITLES: Record<TagToolId, MessageKey> = {
        number: 'multiRename.tags.numbering.title',
        replace: 'multiRename.tags.tools.replace.button',
        format: 'multiRename.tags.tools.format.button',
        fromName: 'multiRename.tags.dialog.fromName',
        case: 'multiRename.tags.dialog.case',
        cleanUp: 'multiRename.tags.dialog.cleanUp',
        extras: 'multiRename.tags.dialog.extras',
        clearField: 'multiRename.tags.dialog.clearField',
    }

    const CASES: readonly { value: CaseTool; label: MessageKey }[] = [
        { value: 'sentence', label: 'multiRename.tags.tools.caseSentence' },
        { value: 'title', label: 'multiRename.tags.tools.caseTitle' },
        { value: 'lower', label: 'multiRename.tags.tools.caseLower' },
        { value: 'upper', label: 'multiRename.tags.tools.caseUpper' },
    ]

    /** Clean up's one-press tools. */
    const PLAIN: readonly { label: MessageKey; tool: TagTool }[] = [
        { label: 'multiRename.tags.tools.stripTrackPrefix', tool: { kind: 'stripTrackPrefix' } },
        { label: 'multiRename.tags.tools.albumArtistFromArtist', tool: { kind: 'albumArtistFromArtist' } },
        { label: 'multiRename.tags.tools.narratorFromText', tool: { kind: 'narratorFromText' } },
        { label: 'multiRename.tags.tools.clearRedundantTitles', tool: { kind: 'clearRedundantTitles' } },
        { label: 'multiRename.tags.tools.discFromFolder', tool: { kind: 'discFromFolder' } },
        { label: 'multiRename.tags.tools.discNumberOnly', tool: { kind: 'discNumberOnly' } },
    ]

    /** The text tools keep their panel open to run again; the rest close once run. */
    const STAYS_OPEN: ReadonlySet<TagToolId> = new Set(['replace', 'format', 'fromName'])

    let checked = $state.raw<readonly string[]>([])
    // Read once: the field is the one the tool opened on.
    // svelte-ignore state_referenced_locally
    let clearing = $state<TagField>(field ?? 'title')

    const caseFieldNames = $derived(formatUnitList(caseFields(field).map((f) => tString(FIELD_LABELS[f]))))
    const fieldItems = $derived(TAG_FIELDS.map((value) => ({ value, label: tString(FIELD_LABELS[value]) })))
    const clearingName = $derived(tString(FIELD_LABELS[clearing]))

    /** The Clean up dialog's Text group: Remove diacritics on all fields but Comment and Description is the default. */
    const ALL_FIELDS = 'all'
    let textOptions = $state<TextOptions>({ greekToLatin: false, removeDiacritics: true, normalizeUnicode: false })
    let textChoice = $state(ALL_FIELDS)
    let textFailed = $state(false)
    const textFieldItems = $derived([
        { value: ALL_FIELDS, label: tString('multiRename.tags.tools.text.allFields') },
        ...(field === null ? [] : [{ value: field, label: tString(FIELD_LABELS[field]) }]),
    ])
    const textToRun = $derived(
        textTool(textOptions, textChoice === ALL_FIELDS || field === null ? DIACRITICS_FIELDS : [field]),
    )

    async function runText(): Promise<void> {
        const tool = textToRun
        if (tool === null) return
        textFailed = false
        const ran = await runScoped((scope) => tags.textTool(tool, scope).then(() => true))
        // Stay open when the run didn't happen or failed, so the notice is seen and Run can be retried.
        if (ran === true && tags.error === null) onClose()
        else if (ran === true) textFailed = true
    }

    function setChecked(key: string, on: boolean): void {
        checked = on ? [...checked, key] : checked.filter((k) => k !== key)
    }

    function run(tool: TagTool | null): void {
        if (tool === null) return
        onClose()
        onTool(tool)
    }

    function clear(): void {
        onClose()
        onClear(clearing)
    }
</script>

{#if open === 'number'}
    <NumberingDialog {trackCount} state={tags} {runScoped} onClose={onClose} />
{:else}
    <ModalDialog titleId="tag-tool-title" containerStyle="width: 480px; max-width: 90vw" growDownward onclose={onClose}>
        {#snippet title()}{tString(TITLES[open])}{/snippet}
        <div class="tool">
            {#if open === 'replace'}
                <ReplacePanel disabled={false} state={tags} {runScoped} focusedField={field} />
            {:else if open === 'format'}
                <FormatPanel disabled={false} state={tags} {runScoped} />
            {:else if open === 'fromName'}
                <NamePatternPanel disabled={false} state={tags} {runScoped} {scopeKey} />
            {:else if open === 'case'}
                <p class="hint">{tString('multiRename.tags.tools.caseFields', { fields: caseFieldNames })}</p>
                <div class="choices">
                    {#each CASES as choice (choice.value)}
                        <Button onclick={() => { run(caseTool(choice.value, field)) }}>{tString(choice.label)}</Button>
                    {/each}
                </div>
            {:else if open === 'cleanUp'}
                <!-- eslint-disable-next-line cmdr/dialog-buttons-in-footer -- each button is a tool that runs and closes, a choice list like Case's, not the dialog's own actions (those are in the footer). -->
                <div class="choices">
                    {#if showsRenumber(trackCount)}
                        <Button onclick={() => { run({ kind: 'renumber', start: 1 }) }}>{tString('multiRename.tags.tools.renumber')}</Button>
                    {/if}
                    {#each PLAIN as plain (plain.label)}
                        <Button onclick={() => { run(plain.tool) }}>{tString(plain.label)}</Button>
                    {/each}
                </div>
                <div class="text" role="group" aria-labelledby="tag-text-group">
                    <h3 id="tag-text-group">{tString('multiRename.tags.tools.text.group')}</h3>
                    <Checkbox checked={textOptions.greekToLatin} onCheckedChange={(on: boolean) => { textOptions = { ...textOptions, greekToLatin: on } }}>
                        {tString('multiRename.greekToLatin')}
                    </Checkbox>
                    <Checkbox checked={textOptions.removeDiacritics} onCheckedChange={(on: boolean) => { textOptions = { ...textOptions, removeDiacritics: on } }}>
                        {tString('multiRename.removeDiacritics')}
                    </Checkbox>
                    <Checkbox checked={textOptions.normalizeUnicode} onCheckedChange={(on: boolean) => { textOptions = { ...textOptions, normalizeUnicode: on } }}>
                        {tString('multiRename.normalizeUnicode')}
                    </Checkbox>
                    <div class="line">
                        <Select
                            items={textFieldItems}
                            value={textChoice}
                            ariaLabel={tString('multiRename.tags.tools.text.fieldsLabel')}
                            onChange={(value: string) => { textChoice = value }}
                        />
                        <Button size="mini" variant="primary" disabled={textToRun === null} onclick={() => void runText()}>
                            {tString('multiRename.tags.tools.text.run')}
                        </Button>
                    </div>
                    {#if textFailed}
                        <p class="error" role="alert">{tString('multiRename.tags.tools.text.failed')}</p>
                    {/if}
                </div>
            {:else if open === 'extras'}
                <ul aria-label={tString('multiRename.tags.tools.extrasLabel')}>
                    {#each extras as key (key)}
                        <li>
                            <Checkbox checked={checked.includes(key)} onCheckedChange={(on: boolean) => { setChecked(key, on) }}>{key}</Checkbox>
                        </li>
                    {/each}
                </ul>
            {:else}
                <div class="line">
                    <Select
                        items={fieldItems}
                        value={clearing}
                        ariaLabel={tString('multiRename.tags.clearField.field')}
                        onChange={(value: string) => { clearing = value as TagField }}
                    />
                </div>
                <p class="hint">
                    {shownCount !== null
                        ? tString('multiRename.tags.clearField.shown', {
                              field: clearingName,
                              count: shownCount,
                              countText: formatInteger(shownCount),
                          })
                        : selectedCount === 0
                        ? tString('multiRename.tags.clearField.all', { field: clearingName })
                        : tString('multiRename.tags.clearField.some', {
                              field: clearingName,
                              count: selectedCount,
                              countText: formatInteger(selectedCount),
                          })}
                </p>
            {/if}
        </div>
        {#snippet footer()}
            {#if STAYS_OPEN.has(open) || open === 'case' || open === 'cleanUp'}
                <Button onclick={onClose}>{tString('multiRename.tags.dialog.close')}</Button>
            {:else if open === 'extras'}
                <Button onclick={onClose}>{tString('multiRename.tags.dialog.close')}</Button>
                <Button variant="primary" disabled={checked.length === 0} onclick={() => { run(removeExtrasTool(checked)) }}>
                    {tString('multiRename.tags.tools.removeChecked', { count: checked.length, countText: formatInteger(checked.length) })}
                </Button>
            {:else}
                <Button onclick={onClose}>{tString('multiRename.tags.dialog.close')}</Button>
                <Button variant="primary" onclick={clear}>{tString('multiRename.tags.clearField.run')}</Button>
            {/if}
        {/snippet}
    </ModalDialog>
{/if}

<style>
    .tool {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .choices,
    .line {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .hint {
        margin: 0;
        color: var(--color-text-secondary);
    }

    .text {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-xs);
    }

    .error {
        margin: 0;
        color: var(--color-error-text);
    }

    h3 {
        margin: 0;
        font-size: inherit;
        font-weight: 600;
    }

    ul {
        margin: 0;
        padding: 0;
        list-style: none;
        max-height: 40vh;
        overflow: auto;
    }
</style>
