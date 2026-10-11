<!--
  KeyHint: the key that presses a button, shown as a small chip after the
  button's label. The one way to put a key inside a control: the F2 chip on the
  Background / Queue buttons and the letters on the decision prompts' buttons.

  Hidden from assistive tech: read as part of the button's name, the key would
  only garble it ("Skip S"). Give the button `aria-keyshortcuts` with the same
  key, which is where a screen reader learns it. `DETAILS.md` § KeyHint.
-->
<script lang="ts">
    import ShortcutChip from './ShortcutChip.svelte'

    interface Props {
        /** The fixed key, as `ShortcutChip`'s literal `key`. */
        key: string
    }

    const { key }: Props = $props()
</script>

<span class="key-hint" aria-hidden="true"><ShortcutChip {key} size="sm" /></span>

<style>
    .key-hint {
        display: inline-flex;
        margin-inline-start: var(--spacing-sm);
        vertical-align: middle;
    }
</style>
