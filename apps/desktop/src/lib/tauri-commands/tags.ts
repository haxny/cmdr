// The deferred, visible-row enrichment commands: macOS Finder color tags
// (toggling a tag across paths, reading fresh tag data into a cached listing)
// and packages macOS reports for dotted folders the package list doesn't know.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '#lib/ipc/bindings.js'
import type { TimedOut } from './ipc-types'

/**
 * Listens for `tags-skipped`: a toggle left `count` files alone because their
 * existing tags couldn't be read or decoded. Emitted by both the keyboard command
 * and the context-menu circles.
 */
export function onTagsSkipped(handler: (count: number) => void): Promise<UnlistenFn> {
  return events.tagsSkipped.listen((event) => {
    handler(event.payload.count)
  })
}

/**
 * Toggles a Finder color tag (`color` 1..=7) across `paths`, then patches the
 * resulting tags into the cached listing so the panes re-render immediately.
 * Off macOS this is a no-op.
 */
export function toggleTags(listingId: string, paths: string[], color: number): Promise<TimedOut<null>> {
  return commands.toggleTags(listingId, paths, color)
}

/**
 * Reads macOS Finder tags for the given paths and patches them into the cached
 * listing, emitting a coalesced `directory-diff` so the panes show the colored
 * dots.
 */
export function enrichTags(listingId: string, paths: string[]): Promise<TimedOut<null>> {
  return commands.enrichTags(listingId, paths)
}

/**
 * Asks macOS which of the given visible paths are packages the backend's package
 * list doesn't know (`Logo.sketchbundle`), and patches them into the cached
 * listing (`isPackage`, a `pkg:` icon), emitting a `directory-diff` for the rows
 * that changed. The backend filters to dotted folder names itself, and does
 * nothing on a non-local volume or off macOS.
 */
export function enrichPackages(listingId: string, paths: string[]): Promise<TimedOut<null>> {
  return commands.enrichPackages(listingId, paths)
}
