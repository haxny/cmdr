<script lang="ts">
    /**
     * The picker of a «multiple values» row: a mini button that opens a list of the distinct values the files hold,
     * each with how many files carry it. Picking one hands its values to `onPick`, which sets them on every file.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { ValueCount } from '#lib/ipc/bindings.js'
    import Button from '#lib/ui/Button.svelte'
    import Popover from '#lib/ui/Popover.svelte'
    import { formatFieldValue } from './shared-field-input'

    interface Props {
        /** The field's display name, like Album. */
        field: string
        choices: ValueCount[]
        disabled: boolean
        onPick: (values: string[]) => void
    }

    const { field, choices, disabled, onPick }: Props = $props()

    let anchor = $state<HTMLElement>()
    let open = $state(false)
    let list = $state<HTMLElement>()

    const label = $derived(tString('multiRename.tags.choices.label', { field }))
    const items = $derived(
        choices.map((choice) => {
            const { text } = formatFieldValue({ same: choice.values })
            return {
                values: choice.values,
                text: tString('multiRename.tags.choices.item', {
                    value: text === '' ? tString('multiRename.tags.choices.empty') : text,
                    countText: formatInteger(choice.files),
                }),
            }
        }),
    )

    function pick(values: string[]): void {
        open = false
        anchor?.focus()
        onPick(values)
    }

    function onOptionKey(event: KeyboardEvent, values: string[]): void {
        if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            pick(values)
            return
        }
        const step = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0
        if (step === 0) return
        event.preventDefault()
        const options = [...(list?.querySelectorAll<HTMLElement>('[role="option"]') ?? [])]
        const at = options.findIndex((option) => option === event.currentTarget)
        options.at((at + step + options.length) % options.length)?.focus()
    }
</script>

<Button
    size="mini"
    {disabled}
    aria-label={label}
    aria-expanded={open}
    onclick={(event: MouseEvent) => {
        anchor = event.currentTarget as HTMLElement
        open = !open
    }}
>
    {tString('multiRename.tags.choices.button')}
</Button>

{#if anchor}
    <Popover {anchor} {open} ariaLabel={label} surface="solid" onClose={() => { open = false }}>
        <ul class="choices" role="listbox" aria-label={label} bind:this={list}>
            {#each items as item, index (index)}
                <li
                    class="choice"
                    role="option"
                    aria-selected="false"
                    tabindex="0"
                    onclick={() => { pick(item.values) }}
                    onkeydown={(event) => { onOptionKey(event, item.values) }}
                >
                    {item.text}
                </li>
            {/each}
        </ul>
    </Popover>
{/if}

<style>
    .choices {
        margin: 0;
        padding: 0;
        list-style: none;
    }

    .choice {
        padding: var(--spacing-xxs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        cursor: default;
    }

    .choice:hover,
    .choice:focus-visible {
        background: var(--color-accent-subtle);
    }
</style>
