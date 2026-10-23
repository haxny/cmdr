/**
 * Astro integration that gives every `public/` asset a content-versioned URL: `/hero/x.webp`
 * becomes `/hero/x.webp?v=3f9a1c2b7d`. Astro already hashes what it bundles (`/_astro/`), but
 * files in `public/` keep their names across deploys, so a browser or CDN that cached one
 * keeps serving the old bytes. With the version in the URL, nginx can cache them for a year
 * and a changed file is a new URL (`nginx.conf` keys its cache header on `?v=`).
 *
 * Runs at `astro:build:done` over the emitted HTML, like `smart-quotes.ts`, because that's the
 * one layer that sees markdown, templates, and the inlined CSS alike. It only rewrites URL
 * attribute values and `url()` inside `<style>`, never text, and only versions a URL whose
 * file exists in the build. `website-dead-links` fails a page that still references an
 * unversioned `public/` asset, so a reference this misses can't silently go stale.
 */

import type { AstroIntegration } from 'astro'
import { createHash } from 'node:crypto'
import { readFile, readdir, stat, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

/** The file types that get versioned: the ones nginx's long-cache location serves. */
export const VERSIONED_EXTENSIONS = [
  'webp',
  'png',
  'jpg',
  'jpeg',
  'gif',
  'svg',
  'ico',
  'avif',
  'mp4',
  'webm',
  'woff',
  'woff2',
  'ttf',
  'otf',
  'eot',
  'js',
  'css',
]

/** A root-relative (or own-origin absolute) URL to a versionable file, at a token boundary, with no query yet. */
const ASSET_URL = new RegExp(
  `(?<![\\w:/.])((?:https://getcmdr\\.com)?/(?!_astro/)[^\\s"'(),?#]+\\.(?:${VERSIONED_EXTENSIONS.join('|')}))(?=$|[\\s,#'")])`,
  'gi',
)

/** Attributes whose value is a URL or a `srcset` list. */
const URL_ATTRIBUTE = /(\s(?:src|srcset|href|content|poster)=)(["'])(.*?)\2/gi

const STYLE_BLOCK = /(<style\b[^>]*>)([\s\S]*?)(<\/style>)/gi

/**
 * Appends `?v=<version>` to every asset URL in `html` that `versionOf` knows. Pure, so the
 * build hook is only file plumbing around it.
 */
export function versionAssetUrls(html: string, versionOf: (path: string) => string | undefined): string {
  const versionUrls = (text: string) =>
    text.replace(ASSET_URL, (url: string) => {
      const version = versionOf(url.replace(/^https:\/\/getcmdr\.com/, ''))
      return version ? `${url}?v=${version}` : url
    })
  return html
    .replace(
      URL_ATTRIBUTE,
      (_match, name: string, quote: string, value: string) => `${name}${quote}${versionUrls(value)}${quote}`,
    )
    .replace(STYLE_BLOCK, (_match, open: string, css: string, close: string) => `${open}${versionUrls(css)}${close}`)
}

async function htmlFilesUnder(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true, recursive: true })
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith('.html'))
    .map((entry) => join(entry.parentPath, entry.name))
}

export function assetVersionsIntegration(): AstroIntegration {
  return {
    name: 'asset-versions',
    hooks: {
      'astro:build:done': async ({ dir, logger }) => {
        const distDir = fileURLToPath(dir)
        // Path → version, or `undefined` for a URL with no file behind it (left as written).
        const versions = new Map<string, string | undefined>()
        const versionOf = async (urlPath: string): Promise<string | undefined> => {
          if (versions.has(urlPath)) return versions.get(urlPath)
          let version: string | undefined
          try {
            const file = join(distDir, decodeURIComponent(urlPath))
            if ((await stat(file)).isFile()) {
              version = createHash('sha256')
                .update(await readFile(file))
                .digest('hex')
                .slice(0, 10)
            }
          } catch {
            version = undefined
          }
          versions.set(urlPath, version)
          return version
        }

        let rewritten = 0
        for (const file of await htmlFilesUnder(distDir)) {
          const html = await readFile(file, 'utf8')
          // Resolve every candidate first, so the rewrite itself stays synchronous and pure.
          for (const match of html.matchAll(ASSET_URL)) await versionOf(match[1].replace(/^https:\/\/getcmdr\.com/, ''))
          const out = versionAssetUrls(html, (path) => versions.get(path))
          if (out !== html) {
            await writeFile(file, out)
            rewritten++
          }
        }
        const versioned = [...versions.values()].filter(Boolean).length
        logger.info(`versioned ${String(versioned)} public asset URL(s) across ${String(rewritten)} page(s)`)
      },
    },
  }
}
