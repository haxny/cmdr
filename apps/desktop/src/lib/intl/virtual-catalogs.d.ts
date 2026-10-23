// The two virtual modules `scripts/vite-locale-catalogs.ts` serves to the app and to Vitest.

declare module 'virtual:cmdr-catalog/en' {
  /** The English catalog: every `messages/en/*.json` merged, `@key` metadata stripped. */
  const catalog: Record<string, string>
  export default catalog
}

declare module 'virtual:cmdr-locale-manifest' {
  /** Every shipped catalog tag, sorted. */
  export const shippedLocales: readonly string[]
  /** tag → the few messages every window needs from every catalog (`MANIFEST_KEYS`). */
  export const manifestMessages: Readonly<Record<string, Readonly<Record<string, string>> | undefined>>
  /** tag → a loader for that catalog's chunk: every shipped tag but `en`, which is always loaded. */
  export const loaders: Readonly<Record<string, () => Promise<{ default: Record<string, string> }>>>
}
