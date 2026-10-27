<script lang="ts">
    /**
     * Rebuilds one tag field from a Multi-Rename mask: `[N]` is the field's current value (`[N3-]` its text from the
     * third character), `[=tag.album]` and the like read the file's other tags. A field with several values (Artist,
     * Genre) keeps only the first, which the hint says. A mask that doesn't parse is worded like Multi-Rename words it.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { TagField } from '#lib/ipc/bindings.js'
    import Button from '#lib/ui/Button.svelte'
    import Select from '#lib/ui/Select.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import { maskErrorText } from '../error-text'
    import { FIELD_LABELS, TAG_FIELDS } from './field-labels'
    import type { TagEditorState } from './tag-editor-state.svelte'
    import type { RunScoped } from './tag-tools'

    interface Props {
        disabled: boolean
        state: TagEditorState
        runScoped: RunScoped
    }

    const { disabled, state: tags, runScoped }: Props = $props()

    let field = $state<TagField>('title')
    let mask = $state('[N]')
    let failure = $state<string | null>(null)

    const items = $derived(TAG_FIELDS.map((value) => ({ value, label: tString(FIELD_LABELS[value]) })))
    const ready = $derived(!disabled && mask !== '')

    async function run(): Promise<void> {
        failure = null
        const ran = await runScoped(async (scope) => {
            await tags.textTool({ kind: 'format', field, mask }, scope)
            return true
        })
        const error = tags.error
        failure = ran === true && error?.kind === 'badMask' ? maskErrorText(error.error) : null
    }
</script>

<div class="panel">
    <div class="line">
        <span class="name">{tString('multiRename.tags.tools.format.field')}</span>
        <Select
            {items}
            value={field}
            {disabled}
            ariaLabel={tString('multiRename.tags.tools.format.field')}
            onChange={(value: string) => { field = value as TagField }}
        />
        <label for="tag-format-mask">{tString('multiRename.tags.tools.format.mask')}</label>
        <TextInput id="tag-format-mask" bind:value={mask} {disabled} invalid={failure !== null} mono />
        <Button size="mini" variant="primary" disabled={!ready} onclick={() => void run()}>
            {tString('multiRename.tags.tools.format.run')}
        </Button>
    </div>
    <p class="hint">{tString('multiRename.tags.tools.format.hint')}</p>
    {#if failure !== null}
        <p class="error" role="alert">{failure}</p>
    {/if}
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

    .name,
    label {
        color: var(--color-text-secondary);
    }

    p {
        margin: 0;
    }

    .hint {
        color: var(--color-text-secondary);
    }

    .error {
        color: var(--color-error-text);
    }
</style>
