<script lang="ts">
    import type { SVGAttributes } from 'svelte/elements'
    import { ICON_COMPONENTS, type IconName } from './icons/icon-map'

    /**
     * The one way to render an inline glyph. Picks the glyph by `name` from the shared registry
     * (`icons/icon-map.ts`) and renders it at `size` px square. Lucide and custom glyphs are
     * interchangeable here, the caller can't tell them apart.
     *
     * Color is intentionally NOT a prop: the glyph inherits `currentColor`, so set `color` on the
     * wrapping element (a `<span>`/`<button>` with a scoped class) the way every call site already
     * does.
     *
     * Decorative by default: with no `aria-label` / `aria-labelledby` the `<svg>` is
     * `aria-hidden="true"`, since an unnamed glyph is noise to a screen reader and the text beside
     * it (or the wrapping button's label) carries the meaning. Pass `aria-label` and it becomes
     * `role="img"` with that name. Every other attribute passes straight through to the `<svg>`
     * via `{...rest}`, so an explicit `aria-hidden` or `role` still wins.
     */
    interface Props extends Omit<SVGAttributes<SVGSVGElement>, 'width' | 'height'> {
        name: IconName
        /** Square size in px (both width and height). */
        size?: number
    }

    const { name, size = 16, ...rest }: Props = $props()

    const Glyph = $derived(ICON_COMPONENTS[name])
    const named = $derived(rest['aria-label'] != null || rest['aria-labelledby'] != null)
</script>

<Glyph
    width={size}
    height={size}
    aria-hidden={named ? undefined : 'true'}
    role={named ? 'img' : undefined}
    {...rest}
/>
