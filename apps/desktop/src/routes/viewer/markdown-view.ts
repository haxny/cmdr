// Pure DOM helpers for the viewer's formatted Markdown view.
//
// The backend renders and sanitizes the document (`src-tauri/src/file_viewer/
// markdown_render.rs`) and leaves every image without a `src`, carrying a
// `data-md-image` marker instead: `local:<token>` (a file on the document's volume),
// `remote:<token>` (a web image, held back until the user allows it), or `blocked`.
// These helpers turn the markers into `cmdr-media://` sources (built only by
// `mediaUrl`) or into small placeholders, after the HTML is in the DOM.

import { mediaUrl } from './media-view'

/** Where a freshly rendered document opens: at a link's target, or back where it was left. */
export type MarkdownLanding = { kind: 'element'; elementId: string | null } | { kind: 'scroll'; top: number }

export type ImageMarker = { kind: 'local'; token: string } | { kind: 'remote'; token: string } | { kind: 'blocked' }

/** The words a placeholder shows, resolved by the caller from the catalog. */
export interface PlaceholderLabels {
  /** A web image held back until the user loads remote images. */
  remote: string
  /** An image the viewer won't load (outside the document's drive, an unknown scheme). */
  blocked: string
  /** A local image that didn't load: missing, or not a picture. */
  missing: string
}

const MARKER_ATTRIBUTE = 'data-md-image'
const PLACEHOLDER_ATTRIBUTE = 'data-md-placeholder'

/** Reads a `data-md-image` value. Anything unexpected is `blocked`: never a guess at a source. */
export function parseImageMarker(value: string | null): ImageMarker {
  const match = /^(local|remote):([0-9a-z]+)$/.exec(value ?? '')
  if (!match) return { kind: 'blocked' }
  return { kind: match[1] as 'local' | 'remote', token: match[2] }
}

/**
 * Wires every marked image under `root`: a local one loads (and turns into a
 * placeholder if it doesn't), a remote or blocked one shows a placeholder. An image
 * with no marker (an inline `data:` picture) is left as rendered.
 */
export function wireImages(root: ParentNode, labels: PlaceholderLabels): void {
  for (const img of root.querySelectorAll<HTMLImageElement>(`img[${MARKER_ATTRIBUTE}]`)) {
    const marker = parseImageMarker(img.getAttribute(MARKER_ATTRIBUTE))
    switch (marker.kind) {
      case 'local':
        load(img, marker.token, labels)
        break
      case 'remote':
        showPlaceholder(img, labels.remote)
        break
      case 'blocked':
        showPlaceholder(img, labels.blocked)
        break
    }
  }
}

/** Puts the held-back web images in, once the backend lets their tokens fetch. */
export function loadRemoteImages(root: ParentNode, labels: PlaceholderLabels): void {
  for (const img of root.querySelectorAll<HTMLImageElement>(`img[${MARKER_ATTRIBUTE}^="remote:"]`)) {
    const marker = parseImageMarker(img.getAttribute(MARKER_ATTRIBUTE))
    if (marker.kind === 'remote') load(img, marker.token, labels)
  }
}

/**
 * The `href` of the link a click landed in, or `null` off a link. The backend decides
 * where it goes (`viewerFollowMarkdownLink`); a link the sanitizer stripped (an unsafe
 * scheme) has no `href` left and goes nowhere.
 */
export function clickedLinkHref(target: EventTarget | null): string | null {
  const link = target instanceof Element ? target.closest('a') : null
  return link?.getAttribute('href') || null
}

/**
 * The element an in-page link lands on, by its rendered id (`md-…`), or `null`. Matched by
 * comparing ids rather than through a selector, so no character in a heading's slug needs
 * escaping.
 */
export function findAnchorTarget(root: ParentNode, elementId: string): Element | null {
  for (const element of root.querySelectorAll('[id]')) {
    if (element.id === elementId) return element
  }
  return null
}

function load(img: HTMLImageElement, token: string, labels: PlaceholderLabels): void {
  removePlaceholder(img)
  img.hidden = false
  img.addEventListener(
    'error',
    () => {
      showPlaceholder(img, labels.missing)
    },
    { once: true },
  )
  img.src = mediaUrl(token)
}

/** Hides `img` behind a small labelled box naming it by its `alt`, if it has one. */
function showPlaceholder(img: HTMLImageElement, label: string): void {
  removePlaceholder(img)
  img.hidden = true
  const alt = img.getAttribute('alt')?.trim() ?? ''
  const placeholder = img.ownerDocument.createElement('span')
  placeholder.setAttribute(PLACEHOLDER_ATTRIBUTE, '')
  placeholder.setAttribute('role', 'img')
  placeholder.textContent = alt ? `${label}: ${alt}` : label
  placeholder.setAttribute('aria-label', placeholder.textContent)
  img.after(placeholder)
}

function removePlaceholder(img: HTMLImageElement): void {
  const next = img.nextElementSibling
  if (next?.hasAttribute(PLACEHOLDER_ATTRIBUTE)) next.remove()
}
