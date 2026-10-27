<script lang="ts">
    /**
     * The play/stop button of a track row: ▶ plays the track so you can hear which one you are naming, ■ stops it.
     * Named "Play {file}" / "Stop {file}" and `aria-pressed` while it plays, so a screen reader hears which row it is.
     */
    import { tString } from '#lib/intl/messages.svelte.js'

    interface Props {
        fileName: string
        playing: boolean
        disabled?: boolean
        onToggle: () => void
    }

    const { fileName, playing, disabled = false, onToggle }: Props = $props()

    const label = $derived(tString(playing ? 'multiRename.tags.play.stop' : 'multiRename.tags.play.play', { fileName }))
</script>

<button
    type="button"
    class="play"
    class:on={playing}
    aria-label={label}
    aria-pressed={playing}
    {disabled}
    title={label}
    onclick={onToggle}
>
    <span aria-hidden="true">{playing ? '■' : '▶'}</span>
</button>

<style>
    .play {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 24px;
        height: 24px;
        padding: 0;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-xs);
        cursor: default;
    }

    .play:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
    }

    .play:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .play.on {
        border-color: var(--color-accent);
    }

    .play:disabled {
        color: var(--color-text-tertiary);
    }
</style>
