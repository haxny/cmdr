<script lang="ts">
    /**
     * Debug > Visual cues: previews the GitHub issue #440 proposals (focused pane, marked files,
     * focus ring) on the main window's file panes, plus simulated Increase contrast and
     * Differentiate without color. Dev-only, nothing persists. The options themselves live in
     * `app-visual-cues.css`, each with its worst-case contrast ratio.
     */
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import RadioGroup from '#lib/ui/RadioGroup.svelte'
    import { getAppLogger } from '#lib/logging/logger.js'
    import {
        applyVisualCueProposals,
        currentVisualCueProposals,
        VISUAL_CUES_EVENT,
        type MarkedCue,
        type PaneCue,
        type RingCue,
        type VisualCueProposals,
    } from '#lib/debug/visual-cue-proposals.js'

    const log = getAppLogger('debug')

    let proposals: VisualCueProposals = $state(currentVisualCueProposals())

    const paneItems = [
        { value: 'off', label: 'Off (today: 1.02:1 light, 1.10:1 dark)' },
        { value: 'header-bar', label: '1a. Header bar', description: 'Bar vs header 5.26 / 5.98' },
        { value: 'cursor-ring', label: '1b. Cursor ring', description: 'Ring vs cursor fill 4.57 / 4.53' },
        { value: 'hollow-unfocused', label: '1c. Hollow unfocused cursor', description: 'Dashes vs pane 3.15 / 3.24' },
    ]
    const markedItems = [
        { value: 'off', label: 'Off (today: red vs text 2.96:1 light, 1.59:1 dark)' },
        { value: 'bold', label: '2a. Heavier text', description: 'Weight, no ratio change' },
        { value: 'edge-bar', label: '2b. Edge bar', description: 'Bar vs row 4.51 / 8.22' },
        { value: 'badge', label: '2c. Mark badge', description: 'Dot vs row 4.51 / 8.22' },
    ]
    const ringItems = [
        { value: 'off', label: 'Off (today: 1.32:1 light, 2.92:1 dark)' },
        { value: 'ink-outside', label: '3a. Accent inside, ink outside', description: 'Ink vs surface 14.60 / 11.71' },
        { value: 'ink-inside', label: '3b. Ink inside, accent outside', description: 'Ink vs surface 14.60 / 11.71' },
        { value: 'strong-accent', label: '3c. Strong accent', description: 'Ring vs surface 4.81 / 5.98' },
    ]

    async function update(patch: Partial<VisualCueProposals>) {
        proposals = { ...proposals, ...patch }
        // This window too, so the focus rings show on the Components catalog.
        applyVisualCueProposals(proposals)
        try {
            const { emitTo } = await import('@tauri-apps/api/event')
            await emitTo('main', VISUAL_CUES_EVENT, proposals)
        } catch (error) {
            log.warn('Could not send the visual cue proposals to the main window: {error}', { error })
        }
    }
</script>

<section class="debug-section">
    <h2>Visual cues (#440)</h2>
    <p class="hint">
        Previews on the main window. Worst ratios are light / dark, across every accent and pane tint. Nothing persists:
        reload to reset.
    </p>

    <h3>1. Focused pane</h3>
    <RadioGroup
        items={paneItems}
        value={proposals.pane}
        onValueChange={(v) => void update({ pane: v as PaneCue })}
        ariaLabel="Focused pane cue"
    />

    <h3>2. Marked files</h3>
    <RadioGroup
        items={markedItems}
        value={proposals.marked}
        onValueChange={(v) => void update({ marked: v as MarkedCue })}
        ariaLabel="Marked file cue"
    />

    <h3>3. Focus ring</h3>
    <RadioGroup
        items={ringItems}
        value={proposals.ring}
        onValueChange={(v) => void update({ ring: v as RingCue })}
        ariaLabel="Focus ring"
    />

    <h3>Simulate macOS display options</h3>
    <div class="toggle-row">
        <span>Increase contrast</span>
        <Checkbox
            checked={proposals.simulateIncreaseContrast}
            onCheckedChange={(checked: boolean) => void update({ simulateIncreaseContrast: checked })}
            ariaLabel="Simulate Increase contrast"
        />
    </div>
    <div class="toggle-row">
        <span>Differentiate without color (turns on 1a and 2b)</span>
        <Checkbox
            checked={proposals.simulateDifferentiateWithoutColor}
            onCheckedChange={(checked: boolean) => void update({ simulateDifferentiateWithoutColor: checked })}
            ariaLabel="Simulate Differentiate without color"
        />
    </div>
</section>

<style>
    .hint {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    h3 {
        margin-top: var(--spacing-md);
    }
</style>
