# MacUpdate listing

Live: https://cmdr.macupdate.com/ (accepted from the 2026-09-18 v0.46.0 submission, the first of three that went
through). Submission form: https://member.macupdate.com/content/submit (needs a MacUpdate member account). The same form
creates and modifies a listing: type `Cmdr` into "Modify an existing listing?" at the top to load this one.

Status: **v0.53.0 prepared 2026-10-15, not yet submitted** (the fields below are the update to paste). The live page
still shows v0.46.0; the v0.47.0 and v0.49.0 updates never went in, and this one supersedes both. Since it skips
v0.47–v0.52, the Description picks up what shipped in between (multi-rename, compare folders, folder sizes on demand,
S3) and the Version changes cover v0.53.0 alone.

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
  Cmdr is in open beta. There might be sharp edges in the newer features (archives, multi-rename, the operation log,
  servers, and AI), but the core is well-tested, stable software used every day by the author and a group of testers.
  Feedback goes straight to the developer!
</p>
<h5>Core features</h5>
<ul>
  <li>
    Two panes, tabs, command palette, keyboard-first. Common shortcuts like F5 to copy, F6 to move, F8 to delete all
    work, and are remappable.
  </li>
  <li>
    Browse, copy, move, rename, delete, compress/decompress with accurate progress bars, honest ETAs, cancellation.
    Optimized for data safety, speed, and transparency.
  </li>
  <li>
    Queue multiple file operations, send any of them to the background, pause/resume file transfers, view a full,
    searchable log of past operations, with rollback for anything that didn't permanently delete data.
  </li>
  <li>
    Total Commander favorites: multi-rename with patterns, counters, and search and replace (with a live preview and one
    Undo), compare folders, and folder sizes on demand.
  </li>
  <li>
    Very fast: Lists 50,000 files near-instantly, and the built-in viewer opens a 10 GB file near-instantly with fast
    search.
  </li>
  <li>
    Accessible: works with VoiceOver, follows the macOS Increase contrast, Differentiate without color, and Show borders
    settings, and everything works from the keyboard.
  </li>
  <li>
    Real dark and light modes, native macOS behavior, and all text color / background contrasts verified against WCAG
    2.2 AA and APCA.
  </li>
</ul>
<h5>Extra features</h5>
<ul>
  <li>
    Full access to Android phones, Kindles, and cameras over MTP and PTP, up to 4x faster than Android File Transfer, no
    hacks needed, works out of the box with any USB cable.
  </li>
  <li>
    Full access to network drives over a custom SMB implementation, roughly 4x faster than the macOS client. Copies
    within one share happen on the NAS itself.
  </li>
  <li>SFTP, WebDAV (Nextcloud and others), and S3 servers (AWS, R2, B2, and more) open like folders.</li>
  <li>
    Keeps a full index of your disk (fully local and private) and uses it to display live folder sizes for
    <em>all</em> your folders, and for near-instant full-drive search. A folder that isn't indexed yet gets walked live,
    with matches arriving as they're found.
  </li>
  <li>For Git repositories, it shows a Git history, branches, worktrees, and stashes browsable like normal folders.</li>
</ul>
<h5>AI features (entirely optional, can be fully local and private with a built-in LLM)</h5>
<ul>
  <li>
    With AI features switched off, Cmdr is a complete Total Commander-style file manager. Many people don't like AI
    features, so they are off by default.
  </li>
  <li>Switched on, it adds natural-language search: "Find my tax report from last year"</li>
  <li>Smart selection: "Select all screenshots in this folder"</li>
  <li>Chat: "Why is my Downloads folder so big?"</li>
  <li>
    Image indexing (fully local and private!): "Find me all photos in this folder where a dog looks into the camera."
  </li>
  <li>
    Natural-language renaming: "Rename all these screenshots based on their content." → The agent can only
    <em>suggest</em> write operations like renames, you are in charge of reviewing and applying them. If you change your
    mind, you can always roll back any past operations.
  </li>
  <li>
    File organization: "Clean up my Downloads folder" and the agent proposes each move for you to approve or reject.
  </li>
  <li>
    It can also watch your folders and raise things on its own: "How about moving that vet invoice to your medical
    papers?" It only ever suggests, you decide, and it stops on one click.
  </li>
  <li>
    The model runs on your Mac by default, so your files and data stay 100% private. You can choose to bring your own
    OpenAI, Claude, Gemini, etc. key, or point Cmdr at any OpenAI-compatible endpoint to use more powerful models.
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
- **Screenshots** (up to five, in this order):
  1. `brand/screenshots/app-main-light.webp`
  2. `brand/screenshots/app-main-dark.webp`
  3. `brand/screenshots/search-light.webp`, `chat-light.webp`, `settings-light.webp` if the listing takes more.
  - Five slots for eight masters, so the dark twins of search, chat, and Settings sit out: the main-view pair already
    shows both themes. The submitted listing still carries only that pair, from before the rest existed.
  - The masters are lossless WebP. MacUpdate's uploader want PNG, so use:
    `magick app-main-light.webp app-main-light.png`. Reshoot per `docs/guides/screenshots.md`.

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
