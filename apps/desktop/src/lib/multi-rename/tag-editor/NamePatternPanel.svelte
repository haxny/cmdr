<script lang="ts">
    /**
     * Reads tags out of the file names. The pattern is Mp3tag-style (`%track% - %title%`) or a regular expression with
     * named groups; Preview lists what it reads from each name (or that it doesn't match) and changes nothing, and
     * Take, which works only on the pattern last previewed, writes those values into the tags. The patterns taken
     * last are listed to pick again. A pattern the backend can't use is worded here, naming the unknown field.
     */
    import { onMount } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { NamePattern, NamePreviewRow } from '#lib/tauri-commands/index.js'
    import Button from '#lib/ui/Button.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import ToggleGroup from '#lib/ui/ToggleGroup.svelte'
    import { FIELD_LABELS } from './field-labels'
    import { loadNamePatterns, rememberNamePattern } from './name-pattern-history'
    import { REGEX_EXAMPLE, patternErrorText } from './pattern-error-text'
    import type { TagEditorState, TagStateError } from './tag-editor-state.svelte'
    import type { RunScoped } from './tag-tools'

    interface Props {
        disabled: boolean
        state: TagEditorState
        runScoped: RunScoped
    }

    const { disabled, state: tags, runScoped }: Props = $props()

    let mode = $state<NamePattern['kind']>('format')
    let text = $state('')
    let history = $state.raw<NamePattern[]>([])
    let previewed = $state.raw<{ pattern: NamePattern; rows: NamePreviewRow[] } | null>(null)
    let failure = $state<string | null>(null)

    const current = $derived<NamePattern>({ kind: mode, pattern: text })
    const fresh = $derived(
        previewed !== null && previewed.pattern.kind === mode && previewed.pattern.pattern === text,
    )
    const edited = $derived(previewed !== null && !fresh)

    const modes = $derived([
        { value: 'format', label: tString('multiRename.tags.tools.name.modePattern') },
        { value: 'regex', label: tString('multiRename.tags.tools.name.modeRegex') },
    ])

    onMount(() => {
        void loadNamePatterns().then((loaded) => {
            // A pattern taken before the list arrives is already in front.
            history = history.length === 0 ? loaded : history
        })
    })

    /** The worded error when it's the pattern's, else nothing (the tab words the rest). */
    function failureOf(error: TagStateError | null): string | null {
        return error?.kind === 'badPattern' ? patternErrorText(error.error) : null
    }

    async function preview(): Promise<void> {
        const asked = current
        failure = null
        const res = await runScoped((scope) => tags.previewFromNames(asked, scope))
        if (res === undefined) return
        if (res.ok) {
            previewed = { pattern: asked, rows: res.value }
        } else {
            previewed = null
            failure = failureOf(res.error)
        }
    }

    async function take(): Promise<void> {
        const asked = current
        failure = null
        const ran = await runScoped(async (scope) => {
            await tags.tool({ kind: 'fromFileName', pattern: asked }, scope)
            return true
        })
        if (ran === undefined) return
        failure = failureOf(tags.error)
        if (failure === null && tags.error === null) history = rememberNamePattern(history, asked)
    }

    function pick(entry: NamePattern): void {
        mode = entry.kind
        text = entry.pattern
    }
</script>

<div class="panel">
    <div class="line">
        <ToggleGroup
            semantics="toggles"
            value={mode}
            options={modes}
            ariaLabel={tString('multiRename.tags.tools.name.mode')}
            {disabled}
            onChange={(value: string) => { mode = value === 'regex' ? 'regex' : 'format' }}
        />
        <TextInput
            id="tag-name-pattern"
            bind:value={text}
            {disabled}
            invalid={failure !== null}
            mono
            ariaLabel={mode === 'regex'
                ? tString('multiRename.tags.tools.name.patternRegex', { example: REGEX_EXAMPLE })
                : tString('multiRename.tags.tools.name.patternPattern')}
            containerStyle="flex: 1; min-width: 200px"
        />
        <Button size="mini" disabled={disabled || text === ''} onclick={() => void preview()}>
            {tString('multiRename.tags.tools.name.preview')}
        </Button>
        <Button size="mini" variant="primary" disabled={disabled || !fresh} onclick={() => void take()}>
            {tString('multiRename.tags.tools.name.take')}
        </Button>
    </div>
    {#if history.length > 0}
        <ul class="recent" aria-label={tString('multiRename.tags.tools.name.recent')}>
            {#each history as entry (`${entry.kind}:${entry.pattern}`)}
                <li>
                    <Button size="mini" {disabled} onclick={() => { pick(entry) }}>{entry.pattern}</Button>
                </li>
            {/each}
        </ul>
    {/if}
    {#if failure !== null}
        <p class="error" role="alert">{failure}</p>
    {:else if edited}
        <p class="hint">{tString('multiRename.tags.tools.name.stale')}</p>
    {/if}
    {#if previewed !== null}
        <div class="scroll">
            <table aria-label={tString('multiRename.tags.tools.name.previewLabel')}>
                <thead>
                    <tr>
                        <th scope="col">{tString('multiRename.tags.tools.name.columnFile')}</th>
                        <th scope="col">{tString('multiRename.tags.tools.name.columnFields')}</th>
                    </tr>
                </thead>
                <tbody>
                    {#each previewed.rows as row (row.path)}
                        <tr>
                            <td class="file">{row.fileName}</td>
                            <td>
                                {#if row.fields === null}
                                    <span class="nomatch">{tString('multiRename.tags.tools.name.noMatch')}</span>
                                {:else}
                                    <ul class="reads">
                                        {#each row.fields as read (read.field)}
                                            <li><span class="field">{tString(FIELD_LABELS[read.field])}</span> {read.value}</li>
                                        {/each}
                                    </ul>
                                {/if}
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    {/if}
</div>

<style>
    .panel {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .line,
    .recent {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .recent {
        margin: 0;
        padding: 0;
        list-style: none;
    }

    p {
        margin: 0;
    }

    .hint,
    .field,
    .nomatch {
        color: var(--color-text-secondary);
    }

    .error {
        color: var(--color-error-text);
    }

    .scroll {
        max-height: 180px;
        overflow: auto;
    }

    table {
        width: 100%;
        border-collapse: collapse;
    }

    th,
    td {
        padding: var(--spacing-xxs) var(--spacing-sm);
        text-align: left;
    }

    .file {
        font-family: var(--font-mono);
    }

    .reads {
        display: flex;
        flex-wrap: wrap;
        gap: 0 var(--spacing-md);
        margin: 0;
        padding: 0;
        list-style: none;
    }
</style>
