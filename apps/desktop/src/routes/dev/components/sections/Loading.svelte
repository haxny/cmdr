<script lang="ts">
    import { onDestroy } from 'svelte'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import LoadingIcon from '#lib/ui/LoadingIcon.svelte'
    import type { ListingPhase } from '#lib/ipc/bindings.js'

    // A restic backup's index load, as the pane shows it before a snapshot's first
    // entries. The static cell is the stable state to judge; the scripted one
    // replays a slow 20 s load on a loop, the shape a NAS repo on a cold cache sends.
    const INDEX_BYTES = 312_000_000
    const INDEX_FILES = 40
    function indexLoad(doneBytes: number): ListingPhase {
        const doneFiles = Math.floor((doneBytes / INDEX_BYTES) * INDEX_FILES)
        return { kind: 'loadingIndex', doneBytes, totalBytes: INDEX_BYTES, doneFiles, totalFiles: INDEX_FILES }
    }
    const staticPhase = indexLoad(48_000_000)

    const scriptStepMs = 250
    const scriptSteps = 20_000 / scriptStepMs
    let scriptStep = $state(0)
    const scriptedPhase = $derived(indexLoad(Math.round((scriptStep / scriptSteps) * INDEX_BYTES)))
    const scriptTimer = setInterval(() => {
        scriptStep = scriptStep >= scriptSteps ? 0 : scriptStep + 1
    }, scriptStepMs)
    onDestroy(() => {
        clearInterval(scriptTimer)
    })
</script>

<SectionCard id="components-loading" label="Loading">
    <div class="grid">
        <div class="cell">
            <p class="caption">Default</p>
            <div class="icon-host">
                <LoadingIcon />
            </div>
        </div>
        <div class="cell">
            <p class="caption">Opening folder</p>
            <div class="icon-host">
                <LoadingIcon openingFolder />
            </div>
        </div>
        <div class="cell">
            <p class="caption">Loaded count</p>
            <div class="icon-host">
                <LoadingIcon loadedCount={1243} />
            </div>
        </div>
        <div class="cell">
            <p class="caption">Finalizing count</p>
            <div class="icon-host">
                <LoadingIcon finalizingCount={5000} />
            </div>
        </div>
        <div class="cell">
            <p class="caption">With cancel hint</p>
            <div class="icon-host">
                <LoadingIcon showCancelHint />
            </div>
        </div>
        <div class="cell wide">
            <p class="caption">Backup index, 48 of 312 MB</p>
            <div class="icon-host">
                <LoadingIcon phase={staticPhase} showCancelHint />
            </div>
        </div>
        <div class="cell wide">
            <p class="caption">Backup index, scripted 20 s load</p>
            <div class="icon-host">
                <LoadingIcon phase={scriptedPhase} showCancelHint />
            </div>
        </div>
    </div>
</SectionCard>

<style>
    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: var(--spacing-lg);
    }

    /* The index load's text and bar need more room than a spinner and a line. */
    .cell.wide {
        grid-column: span 2;
    }

    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    /* LoadingIcon stretches to fill `width: 100%; height: 100%;` so the host
       must give it a finite box. */
    .icon-host {
        height: 120px;
        display: flex;
    }
</style>
