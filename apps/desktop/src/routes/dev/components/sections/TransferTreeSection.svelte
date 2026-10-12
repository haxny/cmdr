<script lang="ts">
    /**
     * The live transfer tree over its six scripted fixtures, each playing at the
     * backend's progress cadence and starting over after it finishes. The shapes
     * the tree must look right on; `lib/file-operations/transfer-tree/fixtures/`.
     */
    import { onDestroy } from 'svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import TransferTree from '$lib/file-operations/transfer-tree/TransferTree.svelte'
    import { TRANSFER_TREE_FIXTURES } from '$lib/file-operations/transfer-tree/fixtures/catalog'
    import { createFixturePlayer } from '$lib/file-operations/transfer-tree/fixtures/fixture-player.svelte'

    const players = TRANSFER_TREE_FIXTURES.map((fixture) => ({ fixture, player: createFixturePlayer(fixture) }))
    for (const { player } of players) player.play()

    onDestroy(() => {
        for (const { player } of players) player.stop()
    })
</script>

<SectionCard id="components-transfer-tree" label="Transfer tree">
    <div class="fixtures">
        {#each players as { fixture, player } (fixture.id)}
            <div class="fixture">
                <span class="fixture-label">{fixture.title} · {fixture.operation}</span>
                <!-- A fresh tree per run: a tree's source is fixed for its life. -->
                {#key player.props.source}
                    <TransferTree
                        source={player.props.source}
                        tick={player.props.tick}
                        operation={player.props.operation}
                    />
                {/key}
            </div>
        {/each}
    </div>
</SectionCard>

<style>
    .fixtures {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(480px, 1fr));
        gap: var(--spacing-xl);
    }

    .fixture {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .fixture-label {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
