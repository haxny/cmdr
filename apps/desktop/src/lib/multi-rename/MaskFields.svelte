<script lang="ts">
    /**
     * The sheet's two mask fields, and the name mask's syntax switch beside its label: "Cmdr" (the
     * `[N]`-style placeholders, with their ▾ editors) or "foobar2000" (a foobar title-formatting
     * pattern as written in foobar; no ▾ editors, since its `[ ]` are optional sections). A foobar
     * pattern's error underlines the character it breaks at. The extension mask is always Cmdr's.
     */
    import ToggleGroup, { type ToggleGroupOption } from '#lib/ui/ToggleGroup.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MaskSyntax } from '#lib/ipc/bindings.js'
    import type { MultiRenameState } from './multi-rename-state.svelte'
    import type { FieldHistoryMenu } from './field-history-menu.svelte'
    import { patternErrorAt } from './error-text'
    import MaskInput from './MaskInput.svelte'
    import FieldHistoryChevron from './FieldHistoryChevron.svelte'

    interface Props {
        tool: MultiRenameState
        fieldHistory: FieldHistoryMenu
        nameMaskInput?: HTMLInputElement
        extensionMaskInput?: HTMLInputElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        tool,
        fieldHistory,
        nameMaskInput = $bindable(),
        extensionMaskInput = $bindable(),
    }: Props = $props()
    /* eslint-enable prefer-const */

    const syntax = $derived<MaskSyntax>(tool.spec.maskSyntax ?? 'cmdr')
    const syntaxOptions = $derived<ToggleGroupOption[]>([
        { value: 'cmdr', label: tString('multiRename.syntax.cmdr'), tooltip: tString('multiRename.syntax.cmdrTooltip') },
        {
            value: 'foobar',
            label: tString('multiRename.syntax.foobar'),
            tooltip: tString('multiRename.syntax.foobarTooltip'),
        },
    ])
    const specError = $derived(tool.error?.type === 'spec' ? tool.error.error.type : null)
    const errorAt = $derived(patternErrorAt(tool.error, tool.spec.nameMask))
</script>

<div class="masks">
    <div class="field grow">
        <div class="label-row">
            <span class="label">{tString('multiRename.nameMask')}</span>
            <ToggleGroup
                semantics="toggles"
                value={syntax}
                options={syntaxOptions}
                onChange={(next: string) => { tool.update({ maskSyntax: next as MaskSyntax }) }}
                ariaLabel={tString('multiRename.syntax.label')}
            />
        </div>
        <MaskInput
            bind:inputElement={nameMaskInput}
            value={tool.spec.nameMask}
            onValueChange={(nameMask: string) => { tool.update({ nameMask }) }}
            ariaLabel={tString('multiRename.nameMask')}
            invalid={specError === 'nameMask' || specError === 'pattern'}
            tokenEditors={syntax === 'cmdr'}
            {errorAt}
        >
            {#snippet trailing()}
                <FieldHistoryChevron
                    disabled={fieldHistory.historyOf('nameMask').length === 0}
                    expanded={fieldHistory.openField === 'nameMask'}
                    onOpen={() => { fieldHistory.open('nameMask') }}
                />
            {/snippet}
        </MaskInput>
    </div>
    <label class="field extension">
        <span class="label">{tString('multiRename.extensionMask')}</span>
        <MaskInput
            bind:inputElement={extensionMaskInput}
            value={tool.spec.extensionMask}
            onValueChange={(extensionMask: string) => { tool.update({ extensionMask }) }}
            ariaLabel={tString('multiRename.extensionMask')}
            invalid={specError === 'extensionMask'}
        >
            {#snippet trailing()}
                <FieldHistoryChevron
                    disabled={fieldHistory.historyOf('extensionMask').length === 0}
                    expanded={fieldHistory.openField === 'extensionMask'}
                    onOpen={() => { fieldHistory.open('extensionMask') }}
                />
            {/snippet}
        </MaskInput>
    </label>
</div>

<style>
    .masks {
        grid-column: span 2;
        display: flex;
        gap: var(--spacing-md);
        align-items: flex-end;
        min-width: 0;
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .grow {
        flex: 1 1 0;
    }

    /* An extension mask is short: `[E]`, or a counter at most. */
    .extension {
        flex: 0 0 140px;
    }

    /* The label on the left, the syntax switch at the field's right edge, on the label's line. */
    .label-row {
        display: flex;
        align-items: flex-end;
        justify-content: space-between;
        gap: var(--spacing-sm);
        min-width: 0;
    }

    .label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
