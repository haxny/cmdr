<script lang="ts">
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import Button from '#lib/ui/Button.svelte'
    import LiveAnnouncer from '#lib/ui/LiveAnnouncer.svelte'
    import LabeledRow from '../LabeledRow.svelte'

    let announcer: { announce: (message: string) => void } | undefined = $state()
    let last = $state('')
    let count = 0

    function say(message: string): void {
        announcer?.announce(message)
        last = message
    }
</script>

<!-- The region is screen-reader only, so the row echoes what it last said. Turn VoiceOver on
     to hear it; pressing the same button twice says it twice. -->
<SectionCard id="components-live-announcer" label="Live announcer">
    <LiveAnnouncer bind:this={announcer} />
    <div class="rows">
        <LabeledRow label="announce">
            <Button size="mini" onclick={() => { say('Left pane, Documents') }}>Same message</Button>
            <Button size="mini" onclick={() => { count++; say(`${String(count)} selected`) }}>Counter</Button>
        </LabeledRow>
        <LabeledRow label="last said">
            <span class="last">{last || '(nothing yet)'}</span>
        </LabeledRow>
    </div>
</SectionCard>

<style>
    .rows {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .last {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
