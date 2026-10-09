<script lang="ts">
    /**
     * The small button at the end of a Multi-Rename text field that opens the
     * field's history, for the pointer; the keyboard has ⌥⇧↓.
     */
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        onopen: () => void
    }

    const { onopen }: Props = $props()
</script>

<Button size="mini" aria-label={tString('multiRename.history')} tooltipContent={tString('multiRename.historyTooltip')} onclick={onopen}>
    <Icon name="chevron-down" size={12} />
</Button>
