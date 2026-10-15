# MacUpdate listing

Live: https://cmdr.macupdate.com/ (accepted from the 2026-09-18 v0.46.0 submission, the first of three that went
through). Submission form: https://member.macupdate.com/content/submit (needs a MacUpdate member account). The same form
creates and modifies a listing: type `Cmdr` into "Modify an existing listing?" at the top to load this one.

Status: **v0.53.0 submitted 2026-10-15, reviewer pinged the same day, awaiting review** (the fields below are what went
in). Before it, the live page showed v0.46.0; the v0.47.0 and v0.49.0 updates were prepared but never submitted.

❗ **A submission sits unreviewed until David pings a MacUpdate reviewer directly.** The v0.46.0 one only went through
after a ping, and the earlier ones never did. The contact is in the vault's directory blitz plan (MacUpdate entry), so
every handoff ends with "submit, then ping". Since it skips v0.47–v0.52, the Description picks up what shipped in
between (multi-rename, compare folders, folder sizes on demand, S3) and the Version changes cover v0.53.0 alone.

❗ **The Download URL must be a DIRECT link to the installer**, not a redirect. Their guidelines say "the direct URL to
the installer package (e.g., .pkg .dmg, .zip)", and `getcmdr.com/download/latest/universal` takes 3 redirects to a
signed `release-assets.githubusercontent.com` blob URL that doesn't end in `.dmg` (verified 2026-09-18). The two
submissions that used the redirect were never accepted and the one with the version-pinned GitHub Releases URL was, so
keep the pinned URL, at the cost of the `?ref=macupdate.com` attribution and a bump per release.

"Version changes" covers the release being submitted (the whole minor line, patches included, once patches start getting
skipped here); the Description carries everything older.

Refresh cadence and what to update per release: `docs/guides/releasing.md` § "Refreshing the app-directory listings".

The Description and Version changes fields take **HTML**, not plain text: `<p>`, `<strong>`, `<h5>`, `<ul>`, `<li>`.
Their own hint says to keep pricing and promotional text out of the description, so the license and price story lives in
the Price field and the note to the review team instead.

## 1. App and developer info

- **App name**: `Cmdr`
- **Developer name**: `David Veszelovszki`
- **Download URL**: `https://github.com/vdavid/cmdr/releases/download/v0.53.0/Cmdr_0.53.0_universal.dmg`
  - Version-pinned, so bump it with every listing update (see the Download URL note at the top). The redirecting
    `https://getcmdr.com/download/latest/universal?ref=macupdate.com` would never go stale and would attribute downloads
    to MacUpdate, but it's what the rejected submissions used.
- **Product page URL**: `https://getcmdr.com`
- **Purchase URL**: `https://getcmdr.com/pricing`
- **Developer support URL**: `https://github.com/vdavid/cmdr/issues`
- **Version number**: `0.53.0`
- **Price**: leave empty (their hint says empty means free). Cmdr is free for personal use; commercial licenses are sold
  on the purchase URL and explained to the review team below.

## 2. Details and description

### Short description

Their hint: a brief, compelling overview of the key value proposition, without the app name.

```
A blazing-fast, keyboard-driven two-pane file manager for macOS, with fully optional, privacy-first AI built in.
```

### Description

```html
<p>
  <strong>Cmdr</strong> brings the Total Commander experience to macOS, and (optionally, only if you enable it) adds AI
  features that genuinely help. Built with Rust, it's extremely fast and respectful toward your CPU, RAM, and disk.
</p>
<p>
  Cmdr is in open beta. It is well-tested, stable software used every day by the author and hundreds of early adopters.
  Feedback goes straight to the developer!
</p>
<h5>Core features</h5>
<ul>
  <li>
    <strong>Basics:</strong> Two panes, tabs, command palette, keyboard-first. Common shortcuts like F5 to copy, F6 to
    move, F8 to delete all work, and are remappable.
  </li>
  <li>
    <strong>Operations:</strong> Browse, copy, move, rename, delete, compress/decompress with accurate progress bars,
    honest ETAs, cancellation. Optimized for data safety, speed, and transparency.
  </li>
  <li>
    <strong>Advanced operations:</strong> Queue multiple file ops, send any of them to the background, pause/resume file
    transfers, view a full, searchable log of past operations, with rollback for anything that didn't permanently delete
    data.
  </li>
  <li><strong>Multi-rename:</strong> With patterns, counters, and search and replace, live preview, Undo.</li>
  <li>
    <strong>Speed:</strong> Lists 50,000 files near-instantly, the built-in viewer opens a 10 GB file in a second,
    near-instant search.
  </li>
  <li>
    <strong>Accessibility:</strong> Follows macOS accessibility settings, works with VoiceOver, has WCAG 2.2 AA and
    APCA-verified contrasts, and everything works with the keyboard.
  </li>
  <li><strong>Design:</strong> Looks good, has real dark and light modes, follows native macOS look and behavior.</li>
</ul>
<h5>Extra features</h5>
<ul>
  <li>
    <strong>Phones:</strong> Full access to Android phones, Kindles, and cameras over MTP and PTP, up to 4x faster than
    Android File Transfer, no hacks needed, works out of the box with any USB cable. Also supports ADB over USB, and
    handles iPhones natively.
  </li>
  <li>
    <strong>SMB:</strong> Full access to network drives over a custom SMB implementation, which is up to 100x faster
    than the macOS client. Copies within a share happen in-place on the server.
  </li>
  <li>
    <strong>More servers:</strong> Full support for SFTP, WebDAV (Nextcloud and others), and S3 servers (AWS, R2, B2,
    and more), including cost estimates for S3 transfers.
  </li>
  <li>
    <strong>Indexing:</strong> (Optional) Keeps a (local&amp;private) full index of your disk to display live folder
    sizes and for near-instant search.
  </li>
  <li>
    <strong>Git:</strong> In Git repos, the commits, branches, worktrees, and stashes are browsable like normal folders.
  </li>
</ul>
<h5>AI features (entirely optional, can be fully local and private with a built-in LLM)</h5>
<ul>
  <li>
    <strong>Fully optional!</strong> With AI features switched off, Cmdr is a complete Total Commander-style file
    manager. Many people don't like AI features, and they are off by default.
  </li>
  <li>
    <strong>Model support:</strong> The default model runs on your Mac, so your data stays 100% private. You can choose
    to bring your own OpenAI, Claude, Gemini, etc. key, or point Cmdr at any OpenAI-compatible endpoint (like your
    ollama server) to use better models.
  </li>
  <li>
    <strong>Built-in chat:</strong> You can ask "Why is my Downloads folder so big?", "Summarize these three docs", etc.
    (alpha version)
  </li>
  <li><strong>Natural-language search:</strong> "Find my tax report from last year" just works.</li>
  <li><strong>Smart selection:</strong> "Select all screenshots in this folder" selects your screenshots.</li>
  <li>
    <strong>Image indexing:</strong> (fully local and private!) "Find me all photos in this folder where a dog looks
    into the camera."
  </li>
  <li>
    <strong>Natural-language renaming:</strong> "Rename all these screenshots based on their content." The agent
    <strong>never</strong> actually renames stuff. It can only <em>suggest</em> write operations like renames! You are
    in charge of reviewing and applying them. And if you change your mind later, you can always roll back any past
    operations.
  </li>
  <li>
    <strong>File organization:</strong> "Clean up my Downloads folder" and the agent proposes each move for you to
    approve or reject.
  </li>
  <li>
    <strong>Proactive suggestions:</strong> The agent can also watch your folders and raise things on its own: "How
    about moving that vet invoice to your donkey's medical papers?" The agent can only <em>suggest</em> actions, you
    need to approve/reject.
  </li>
</ul>
<p>Cmdr is source-available under the Business Source License 1.1: https://github.com/vdavid/cmdr</p>
```

### Version changes

Their hint asks for the changes in the current version, with `<h5>` section heads and `<ul>` lists. This covers v0.53.0
(no patches); what shipped between the live v0.46.0 and it went into the Description instead.

```html
<h5>New</h5>
<ul>
  <li>
    Cmdr works with VoiceOver: file rows say their name, kind, position, and selection, and menus, Settings, search, the
    command palette, notifications, and progress bars speak as you go.
  </li>
  <li>Follows the macOS Increase contrast, Differentiate without color, and Show borders settings live.</li>
  <li>
    Everything works from the keyboard: reach a pane's tabs, location bar, and sort buttons with ⌃F5, notifications with
    ⌃F6, and move tabs and the pane divider with keys.
  </li>
  <li>Edit &gt; Undo move to trash, and a setting that keeps notifications until you close them.</li>
  <li>Copying a file within one SMB share happens on the NAS itself: a 4 GB duplicate takes seconds.</li>
  <li>F9 opens a terminal in the current folder.</li>
  <li>Right-click a job in the operation queue for its actions.</li>
  <li>Help &gt; Accessibility guide.</li>
</ul>
<h5>Improved</h5>
<ul>
  <li>Selected files show an edge bar and heavier text, so they don't rely on color alone.</li>
  <li>Text size goes up to 200%, and small buttons are easier to hit.</li>
  <li>The git commit browser lists up to 100,000 commits in about a second.</li>
  <li>Big SMB folders show their loading count climb, and stop loading when you leave.</li>
  <li>Opening a file says which app it opens in.</li>
</ul>
<h5>Fixed</h5>
<ul>
  <li>Reconnecting to a NAS that accepted the connection but never answered could hang.</li>
  <li>Renaming a file on an SMB share to another letter case was refused, or deleted the file when overwriting.</li>
  <li>Large copies to Nextcloud behind a proxy failed partway.</li>
  <li>A pane inside a zip didn't show changes until you left it, and edited zips kept their old date.</li>
  <li>Type-to-jump and the quick filter didn't start on Cyrillic, Greek, and other non-Latin keyboard layouts.</li>
  <li>The bottoms of letters like "g" and "p" were cut off in the file list.</li>
</ul>
<h5>Security</h5>
<ul>
  <li>Updates must carry Cmdr's Apple Developer ID signature, beside the update signature.</li>
  <li>A backup update-signing key lets a leaked or lost key be replaced with one release.</li>
  <li>A hostile file open in the viewer can't change settings.</li>
  <li>SMB share passwords stay out of logs.</li>
</ul>
```

### System requirements

```
macOS 12 Monterey or later, both Apple Silicon and Intel. macOS 10.15 Catalina and 11 Big Sur run on a best-effort basis.
```

## 3. Media

- **Icon**: `brand/logos/cmdr-512.png` (512x512, transparent)
- **Screenshots** (five, in this order): PNG conversions of the lossless WebP masters, since MacUpdate's uploader wants
  PNG. Run from the repo root; the output folder is gitignored, so the PNGs never get committed:

  ```bash
  magick brand/screenshots/app-main-light.webp brand/listings/macupdate-media/1-app-main-light.png
  magick brand/screenshots/app-main-dark.webp brand/listings/macupdate-media/2-app-main-dark.png
  magick brand/screenshots/search-light.webp brand/listings/macupdate-media/3-search-light.png
  magick brand/screenshots/chat-light.webp brand/listings/macupdate-media/4-chat-light.png
  magick brand/screenshots/settings-light.webp brand/listings/macupdate-media/5-settings-light.png
  ```

  - Five slots for eight masters, so the dark twins of search, chat, and Settings sit out: the main-view pair already
    shows both themes. The submitted listing still carries only that pair, from before the rest existed.
  - Re-run the commands for every listing update, since the masters change with each reshoot
    (`docs/guides/screenshots.md`).

## Comments for the review team

For an update, keep it short:

```
Hi folks! Thanks for listing Cmdr! This is the v0.53.0 update: new version number, download link, version changes, and a refreshed description (accessibility, plus features that shipped since my last update). Thanks!

David
```

The note that went with the first (accepted) submission, kept for reference:

```
Hi folks!

(Sorry, but it's 2026, so I must note that this is 100% human-written content here. ↓ )

Cmdr is a two-pane file manager for macOS, written in Rust, in open beta. I'd love to get some early users to test-run it. It's a free app for individuals. Thanks for your work at MacUpdate!

I've submitted Cmdr twice before (most recently on 2026-07-29 I think) and didn't hear back either time, and I can't find a listing for Cmdr. If something in those submissions was missing, unclear, or disqualifying, I'd appreciate a line about what to fix. Happy to correct anything or provide whatever else you need.

My notes:
- The download URL points straight at the signed DMG on GitHub Releases. The app is dev ID signed and notarized by Rymdskottkärra AB (my Swedish company), and ships as a universal binary.
- I left the price empty because, as I said above, Cmdr is free for personal use. Work use needs a license ($59 once, 1 year of updates), see https://getcmdr.com/pricing.
- Source is available under BSL 1.1 at https://github.com/vdavid/cmdr.
- Anything you need from me, write to me at hello@getcmdr.com and I'll answer usually the same day.

Thanks for the review!
David
```
