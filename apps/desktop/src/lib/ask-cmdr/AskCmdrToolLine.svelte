<!--
  One tool call the assistant made, as one calm row: a glyph, the label naming what the call
  looked at, a "same as above" tag on a repeat, and what came back.

  Every path in the label is a link that shows it in a pane. Paths, names, and queries are
  attacker-controlled (a crafted filename), so they render as plain text through Svelte's
  escaping interpolation and `<Trans>` snippets, never {@html}. Not a live region: a 12-call
  turn would chatter; the bubble's last prose is the one that announces.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { conjunctionListParts } from '#lib/intl/list-format.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { lineLabelText, toolLineLabel, toolOutcomeText, type LinePlace } from './ask-cmdr-labels'
    import { outcomeOk, showsRepeatTag, type RevealPath } from './ask-cmdr-tool-line'
    import { returnFocusToPane, type RailToolCall } from './ask-cmdr-trigger.svelte'

    interface Props {
        tool: RailToolCall
        /** Shows a path in a pane; `+page.svelte` implements it over the explorer. */
        revealPath: RevealPath
    }
    const { tool, revealPath }: Props = $props()

    const uid = $props.id()
    const repeatId = `${uid}-repeat`

    // A settled call with no recorded outcome reads as done, not refused: its note says the rest.
    const ok = $derived(tool.outcome === null || outcomeOk(tool.outcome))
    const label = $derived(toolLineLabel(tool))
    const labelText = $derived(lineLabelText(label))
    const outcome = $derived(toolOutcomeText(tool))
    const repeat = $derived(showsRepeatTag(tool))

    /** Show the place in a pane, then hand focus back to the panes, where the user is now looking. */
    async function show(place: LinePlace): Promise<void> {
        await revealPath({ path: place.path, kind: place.kind })
        returnFocusToPane()
    }
</script>

{#snippet placeLink(place: LinePlace)}
    {#if place.linked}
        <span class="place" use:tooltip={place.path}>
            <LinkButton
                aria-label={tString('askCmdr.tool.showInPane', { path: place.path })}
                aria-describedby={repeat ? repeatId : undefined}
                onclick={() => void show(place)}>{place.shown}</LinkButton
            >
        </span>
    {:else}
        <span class="place">{place.shown}</span>
    {/if}
{/snippet}

<!-- The `<place>` tag: the one path the message names. The tag's own text is the same name. -->
{#snippet placeTag(_children: Snippet)}
    <!-- eslint-disable-next-line @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
    {#if label.place}{@render placeLink(label.place)}{/if}
{/snippet}

<!-- The `<places>` tag: each path a link, joined by the language's own "and". -->
{#snippet placesTag(_children: Snippet)}
    {#each conjunctionListParts(label.places.map((place) => place.shown)) as part, index (index)}
        <!-- eslint-disable-next-line @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
        {#if part.kind === 'item'}{@render placeLink(label.places[part.index])}{:else}{part.text}{/if}
    {/each}
{/snippet}

<div class="tool-line">
    <span class="glyph">
        {#if tool.running}
            <Spinner size="sm" />
        {:else if ok}
            <Icon name="circle-check" size={13} aria-hidden="true" />
        {:else}
            <Icon name="circle-x" size={13} aria-hidden="true" />
        {/if}
    </span>
    <span class="label" use:tooltip={{ text: labelText, overflowOnly: true }}>
        <Trans key={label.key} params={label.params} snippets={{ place: placeTag, places: placesTag }} />
    </span>
    {#if repeat}
        <span class="repeat" id={repeatId}>{tString('askCmdr.tool.repeat')}</span>
    {/if}
    {#if outcome !== null}
        <span class="outcome" use:tooltip={{ text: outcome, overflowOnly: true }}>{outcome}</span>
    {/if}
</div>

<style>
    .tool-line {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        min-height: 28px;
        padding: var(--spacing-xxs) var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .glyph {
        display: flex;
        width: 16px;
        justify-content: center;
        flex: none;
        color: var(--color-text-tertiary);
    }

    /* The label gives way first: a long path ellipsizes before what came back does. */
    .label {
        flex: 0 1 auto;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .place {
        font-family: var(--font-mono);
    }

    .repeat {
        flex: none;
        padding: 0 var(--spacing-xxs);
        border-radius: var(--radius-xs);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .outcome {
        flex: none;
        max-width: 60%;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-tertiary);
    }
</style>
