<script lang="ts">
    /**
     * A picture or PDF, and what covers it while its bytes make their way from a volume.
     * The view stays mounted beneath the panel: its `<img>` or `<embed>` is what's asking.
     * `createViewerPull` decides when the panel shows.
     */
    import MediaImageView from './MediaImageView.svelte'
    import MediaPdfView from './MediaPdfView.svelte'
    import PullProgressPanel from './PullProgressPanel.svelte'
    import type { createViewerPull } from './viewer-pull.svelte'

    interface Props {
        kind: 'image' | 'pdf'
        src: string
        fileName: string
        load: ReturnType<typeof createViewerPull>
        onCancel: () => void
    }

    const { kind, src, fileName, load, onCancel }: Props = $props()

    let imageView = $state<{ handleKey: (e: KeyboardEvent) => boolean }>()

    /** The image's keys; see `MediaImageView.handleKey`. */
    export function handleKey(e: KeyboardEvent): boolean {
        return imageView?.handleKey(e) ?? false
    }
</script>

<div class="media-stage">
    {#if kind === 'image'}
        <MediaImageView bind:this={imageView} {src} {fileName} />
    {:else}
        <MediaPdfView {src} {fileName} arriving={load.active} />
    {/if}
    {#if load.visible}
        <div class="media-arriving">
            <PullProgressPanel
                {fileName}
                bytesDone={load.bytesDone}
                bytesTotal={load.bytesTotal}
                fraction={load.fraction}
                stalled={load.stalled}
                {onCancel}
            />
        </div>
    {/if}
</div>

<style>
    .media-stage {
        position: relative;
        display: flex;
        flex: 1;
        min-height: 0;
    }

    .media-arriving {
        position: absolute;
        inset: 0;
        display: flex;
        background-color: var(--color-bg-primary);
    }
</style>
