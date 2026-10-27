<script lang="ts">
    /**
     * One tag box that commits on Enter or blur and reverts on Esc: the cell of the shared-fields table and of the
     * track list. It sends nothing unless the text differs from what the box held when it was last in step with the
     * model, so focusing and leaving a box, or pressing Enter on it untouched, never writes (a «multiple values» box
     * starts blank and stays blank). Enter never reaches the sheet's own Enter, which starts the rename.
     */
    import { untrack } from 'svelte'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import TextArea from '#lib/ui/TextArea.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'

    interface Props {
        id: string
        /** The model's text for this box. */
        value: string
        ariaLabel?: string
        placeholder?: string
        multiline?: boolean
        disabled?: boolean
        /** Called with the typed text, only when it changed. A returned promise settles when the model has answered. */
        onCommit: (text: string) => void | Promise<void>
    }

    const { id, value, ariaLabel, placeholder, multiline = false, disabled = false, onCommit }: Props = $props()

    let draft = $state(untrack(() => value))
    /** The text the model holds, or the last text sent: what a commit is compared with. */
    let committed = untrack(() => value)
    let seen = untrack(() => value)

    // A new model text replaces the box (an edit that came back, a tool, an undo); the same text leaves a half-typed box alone.
    $effect.pre(() => {
        if (value === seen) return
        seen = value
        committed = value
        draft = value
    })

    function commit(): void {
        if (draft === committed) return
        committed = draft
        // Once the model has answered, the box is compared with what it holds: a refused edit leaves the old
        // text there, so retyping the same text sends it again.
        void Promise.resolve(onCommit(draft)).finally(() => {
            committed = value
        })
    }

    function onkeydown(e: KeyboardEvent): void {
        if (e.isComposing) return
        // A textarea keeps Enter for new lines; ⌘/Ctrl+Enter commits there.
        if (e.key === 'Enter' && (!multiline || e.metaKey || e.ctrlKey)) {
            claimKey(e)
            commit()
        } else if (e.key === 'Escape' && draft !== committed) {
            claimKey(e)
            draft = committed
        }
    }
</script>

{#if multiline}
    <TextArea {id} bind:value={draft} {ariaLabel} {placeholder} {disabled} rows={3} onblur={commit} {onkeydown} />
{:else}
    <TextInput {id} bind:value={draft} {ariaLabel} {placeholder} {disabled} onblur={commit} {onkeydown} />
{/if}
