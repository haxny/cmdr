<script lang="ts">
    /**
     * PathPills: A path rendered as a strip of clickable ancestor pills.
     *
     * Replaces the flat `parentPath` string in `SearchResults` rows. Clicking a segment
     * navigates the active pane to that ancestor folder AND closes the dialog (the parent
     * wires both via `onPick`).
     *
     * The strip NEVER wraps to two lines. When the full path doesn't fit its container,
     * the middle pills collapse into a single "…" pill; hovering that pill shows a tooltip
     * listing the hidden pills, with the same nav-to-ancestor behavior on click. Measurement
     * uses `@chenglou/pretext` via `createPretextMeasure` so we get pixel-accurate widths
     * without DOM reflow.
     *
     * Load-bearing rules:
     *   - The strip is `aria-hidden` plain spans, ❌ never buttons: a row is a `role="option"`,
     *     whose children are presentational, and Tab stops would break its arrow-key flow. The
     *     row reads the path from the `sr-only` copy, and the keyboard route to a result's
     *     folder is Enter on the row, then going up. `lib/query-ui/DETAILS.md`.
     *   - macOS and Linux only: split strictly on `/`. No `\` handling.
     *   - Pill chrome: `--radius-sm`, `--spacing-xxs / --spacing-xs` padding,
     *     `--font-size-xs`, hover background = `--color-bg-tertiary`.
     */
    import { onDestroy, tick } from 'svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { createPretextMeasure } from '#lib/utils/shorten-middle.js'
    import {
        computePathPillsLayout,
        PILL_CHROME_PX,
        PILL_SEPARATOR_GAP_PX,
        scheduleStableWidthMeasure,
        splitPath,
        type Layout,
    } from './path-pills-layout'

    interface Props {
        /** Path to render (typically `entry.parentPath`; may also be the entry's own path). */
        path: string
        /**
         * Called when the user clicks a pill. Receives the absolute path to that ancestor.
         * The parent is expected to navigate the active pane and close the dialog.
         */
        onPick: (path: string) => void
    }

    const { path, onPick }: Props = $props()

    const segments = $derived(splitPath(path))

    // ── Measure-driven collapse ────────────────────────────────────────────────
    //
    // We measure the rendered text widths of every segment (plus separators and an
    // ellipsis pill) and figure out how many middle pills to hide so the strip fits
    // the column. Pretext is async; until it loads we render every segment with CSS
    // `flex-wrap: nowrap` + `overflow: hidden`, so the worst case before measurement
    // is a horizontally-clipped strip — never a two-line wrap.

    let container: HTMLSpanElement | undefined = $state()
    let containerWidth = $state(0)
    let measureWidth = $state<((text: string) => number) | null>(null)
    let pretextPromise: Promise<typeof import('@chenglou/pretext')> | null = null

    function loadPretext(): Promise<typeof import('@chenglou/pretext')> {
        if (!pretextPromise) {
            pretextPromise = import('@chenglou/pretext')
        }
        return pretextPromise
    }

    /**
     * Read the CSS font string from a DOM element. Mirrors `readFont` in
     * `shorten-middle-action.ts`; the inline `style.font` is empty in some Chromium
     * versions so we synthesize from size + family.
     */
    function readFont(node: HTMLElement): string {
        const style = getComputedStyle(node)
        if (style.font) return style.font
        return `${style.fontSize} ${style.fontFamily}`
    }

    /**
     * Decide which segments stay visible. Delegates to the pure `computePathPillsLayout`
     * helper (see `path-pills-layout.ts`) so the algorithm stays unit-testable with mocked
     * widths instead of forcing a real DOM canvas measurement at test time.
     */
    const layout = $derived.by<Layout>(() =>
        computePathPillsLayout(segments, {
            containerWidth,
            measureWidth,
            separatorWidth: measureWidth ? measureWidth('/') + PILL_SEPARATOR_GAP_PX : 0,
            pillChrome: PILL_CHROME_PX,
        }),
    )

    /**
     * HTML for the `…` pill's tooltip. Hidden pills render as clickable buttons; a
     * delegated `mousedown` handler routes the click back to `onPick`.
     */
    const collapsedTooltipHtml = $derived.by(() => {
        if (layout.collapsed.length === 0) return ''
        const items = layout.collapsed
            .map((seg) => {
                const safeLabel = escapeHtml(seg.label)
                const safePath = escapeHtml(seg.fullPath)
                return `<button type="button" class="hidden-pill" data-path="${safePath}" tabindex="-1">${safeLabel}</button>`
            })
            .join('<span class="hidden-sep" aria-hidden="true">/</span>')
        return `<div class="hidden-pills">${items}</div>`
    })

    function escapeHtml(s: string): string {
        return s
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;')
    }

    /** A press on a pill navigates and stays out of the row's own click; the `…` pill only shows its tooltip. */
    function handleStripClick(e: MouseEvent): void {
        const pill = (e.target as HTMLElement).closest<HTMLElement>('.pill')
        if (!pill) return
        e.stopPropagation()
        if (pill.dataset.path) onPick(pill.dataset.path)
    }

    /**
     * Delegated mousedown handler for the tooltip's hidden-pill buttons. The tooltip
     * body lives in a singleton container appended to `<body>`; we route any click
     * inside a `.hidden-pill` to `onPick(path)`.
     */
    function handleTooltipMouseDown(e: MouseEvent): void {
        const target = (e.target as HTMLElement | null)?.closest('.hidden-pill') as HTMLElement | null
        if (!target) return
        const p = target.dataset.path
        if (!p) return
        e.preventDefault()
        e.stopPropagation()
        onPick(p)
    }

    let resizeObserver: ResizeObserver | undefined
    let cancelStableMeasure: (() => void) | undefined
    let mounted = false
    // Plain `let`, read after an await: a row recycled while pretext loads must not
    // schedule a re-measure its `onDestroy` already ran past (nothing would cancel it).
    let destroyed = false
    $effect(() => {
        if (!container || mounted) return
        mounted = true
        const el = container
        containerWidth = el.clientWidth
        resizeObserver = new ResizeObserver(() => {
            containerWidth = el.clientWidth
        })
        resizeObserver.observe(el)
        void tick().then(async () => {
            const pretext = await loadPretext().catch(() => null)
            if (!pretext || destroyed) return
            measureWidth = createPretextMeasure(readFont(el), pretext)
            containerWidth = el.clientWidth
            // The initial `el.clientWidth` read above can land BEFORE the parent
            // CSS grid track resolves to its final width. Without the follow-up
            // re-measure, the strip first renders the full path (uncollapsed
            // fallback while measureWidth was null), then collapses back to
            // ellipses once `measureWidth` lands but `containerWidth` is still
            // stale-small. Re-read on the next animation frame (when the grid
            // layout has settled) and again ~80 ms later (when fonts and any
            // late style recalculations have settled too). Both reads are cheap;
            // the layout `$derived`
            // re-runs only when `containerWidth` actually changes.
            cancelStableMeasure = scheduleStableWidthMeasure(() => {
                containerWidth = el.clientWidth
            })
        })
        document.addEventListener('mousedown', handleTooltipMouseDown, true)
    })

    onDestroy(() => {
        destroyed = true
        resizeObserver?.disconnect()
        cancelStableMeasure?.()
        document.removeEventListener('mousedown', handleTooltipMouseDown, true)
    })
</script>

{#if segments.length > 0}
    <!-- The row's option reads the whole path from here; the strip below is the mouse's. -->
    <span class="sr-only">{path}</span>
    <span class="path-pills" bind:this={container} aria-hidden="true" onclick={handleStripClick}>
        {#each layout.leading as seg, i (seg.fullPath)}
            {#if i > 0}
                <span class="sep">/</span>
            {/if}
            <span class="pill" data-path={seg.fullPath} use:tooltip={seg.fullPath}>{seg.label}</span>
        {/each}
        {#if layout.collapsed.length > 0}
            {#if layout.leading.length > 0}
                <span class="sep">/</span>
            {/if}
            <span class="pill pill-ellipsis" use:tooltip={{ html: collapsedTooltipHtml }}>…</span>
        {/if}
        {#each layout.trailing as seg, i (seg.fullPath)}
            {#if i > 0 || layout.leading.length > 0 || layout.collapsed.length > 0}
                <span class="sep">/</span>
            {/if}
            <span class="pill" data-path={seg.fullPath} use:tooltip={seg.fullPath}>{seg.label}</span>
        {/each}
    </span>
{/if}

<style>
    .path-pills {
        /* `flex` + `width: 100%` (NOT `inline-flex`): the collapse layout reads
           `container.clientWidth` as the available width. An `inline-flex` box
           shrink-wraps to its content, so that read would return the current
           content width, never the free space in the column. That made the
           collapse self-reinforcing: any pixel of measurement overshoot dropped a
           segment, which shrank the box, which locked in more collapses, so paths
           abbreviated with the column half-empty. Filling the cell makes the
           measured width the stable column width, independent of pill count. */
        display: flex;
        flex-wrap: nowrap;
        align-items: center;
        gap: var(--spacing-xxs);
        min-width: 0;
        width: 100%;
        overflow: hidden;
    }

    .sep {
        color: var(--color-text-tertiary);
        /* Path text reads at --font-size-md (matching Name in the dialog's one-step-larger
           type). The eye reads the path as quickly as the filename, not as a footnote.
           Widths are measured from the rendered font (`readFont` → pretext), so the collapse
           layout self-adjusts to this size; no constant to keep in sync. */
        font-size: var(--font-size-md);
        user-select: none;
    }

    .pill {
        background: transparent;
        border: 0;
        /* Trimmed horizontal padding so the larger font doesn't blow the column;
           vertical padding stays at 0 since the row padding handles vertical rhythm. */
        padding: 0 var(--spacing-xxs);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-md);
        font-family: inherit;
        color: var(--color-text-tertiary);
        line-height: var(--font-line-height-tight);
        white-space: nowrap;
        flex-shrink: 0;
        transition:
            background var(--transition-base),
            color var(--transition-base);
    }

    .pill:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* The collapse pill (`…`) reads as "more here", with a subtly different bg
       so the eye finds it. The tooltip lists the hidden segments. */
    .pill-ellipsis {
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
    }

    /* Styles for the hidden-pill list rendered inside the tooltip body. The
       tooltip module sets the rest of the chrome (frosted-glass surface, radius,
       shadow) on the container; we just lay out the items so they read like the
       regular pills. `:global` because the tooltip body lives in a portal. */
    :global(.cmdr-tooltip .hidden-pills) {
        display: inline-flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xxs);
        max-width: 360px;
    }

    :global(.cmdr-tooltip .hidden-pill) {
        background: transparent;
        border: 0;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-family: inherit;
        color: var(--color-accent-text);
        line-height: var(--font-line-height-tight);
        white-space: nowrap;
        cursor: default;
    }

    :global(.cmdr-tooltip .hidden-pill:hover) {
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
    }

    :global(.cmdr-tooltip .hidden-sep) {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }
</style>
