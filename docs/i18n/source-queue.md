# Source queue

Translators log English-side problems here while translating: ambiguous or inconsistent English, a weak `@key`
description, a missing screenshot, or a rule every language should follow. One bullet per item, at most two lines: the
key(s), what's wrong, the suggested fix. The lead fixes the English, the description, or the shared docs (promoting a
rule into `translation-principles.md` or `translator-instructions.md`), then deletes the entry; git keeps the history.

## Open

- `servers.sheet.nfsExportsEmptyList`: one string covers "the server listed none" and "not asked yet"; split the key if
  the two should read differently. (hu, nl, ru, vi)
- The termbase matcher now skips paths and domains, but it still can't tell one sense of a word from another (verb
  "lists" vs the list, "share it yourself" vs a network share), so each new sense needs a `notMatch` patch. A
  sense-aware matcher needs a design. (hu, ru, sv, zh, zh-Hant)

- `fileExplorer.navigation.forgetConfirmButton`: one button serves the server, share, and saved-password alerts; locales
  that clear a password with another verb (zh `清除`) get a button that doesn't match the title. Consider a separate
  key.
- Cross-language rule proposal: when English prose names a button without quotes (`servers.sheet.addAnywayHelp`,
  `servers.pinHint.body`), a locale may quote it; or quote it in English too.
- `fileExplorer.archivedFile.label`, `askCmdr.sessions.archivedBadge`: both are "Archived" in English, so every locale
  needs a term-consistency allowlist entry. Consider "In cold storage" for the file glyph.
- `fileExplorer.network.share.signIn{Title,Message}`, `fileExplorer.networkMount.signIn{Title,Message}`: the screenshot
  is the servers list, not the calm sign-in screen these strings sit on. Capture that pane (with and without the sheet).
- `fileOperations.transferProgress.stage*` (compress and archive-upload phases, and their `*Step` variants): coupled to
  `transfer-dialog.png`, whose note lists only scanning/paused/queued/finishing. Capture a compress-to-remote run
  showing the "Step 1 of 2" line, and name the compress phases in the note.
- `servers.hub.nearbyGroup`: no screenshot, and the description doesn't say whether a count-last shape is fine (hu, zh,
  and zh-Hant lead with "found nearby"). Recapture the hub with the group header showing, and say the order is free.
- `{localNetwork}` keys (`fileExplorer.pane.directConnectionBlockedByThisMacToast`, `.openLocalNetworkSettings`,
  `servers.refusal.localNetworkHint`): no screenshot of the toast or the Add server hint. Capture both so translators
  can judge the button's width and whether the pane name reads better quoted (zh and zh-Hant quote it, the rest don't).
- `errors.eject.unmountRefusedByProcesses`: zh-Hant's list join can run Latin into Han with no space
  (`cfprefsd和其他程序`). (zh-Hant)
- `fileExplorer.listingStalled.*`, `indexing.overall.*`: no `screenshot`. Capture the stalled-listing pane and the
  checklist with the whole-run line, so locales can judge length and the spinner context. (all)
- `errors.write.insufficientSpace.*`, `fileOperations.errorDialog.copyAnyway`: no screenshot of the dialog with "Copy
  anyway". (all)
- `queryUi.date.preset.thisMonth`, `.lastMonth`: `{month}` arrives nominative, so Slavic locales can't say "from the 1st
  of October" (`с 1 октября`). Pass a preformatted day-and-month. (ru)
- `fileOperations.cancelRollback.reason.unverifiable.named`, `askCmdr.renameUndo.skipReason.unverifiable.named`:
  byte-identical English, but one is about items and the other about files; split the wording or say so. (ru)
- `servers.paneState.cancelCycleTooltip`: "Switch back to retry" doesn't say switch back to what. (ru)
- S3 keys (`servers.sheet.s3*`, `servers.refusal.*` S3 arms, `*shareLink*`, `*coldStorage*`, `fileOperations.s3Cost.*`):
  none has a `screenshot`. Capture the S3 form, a refusal, the share-link submenu, and the cost line. (all)
- `settings.managed.*`, `ai.managed.*`, `askCmdr.error.managedByOrganization`, `onboarding.stepBeta.analyticsManaged`:
  no screenshot of the managed card or the locked rows, so value widths are guesses. Capture them under a policy. (ru)
- `fileExplorer.compareDirectories.*`: no screenshot of the compare toasts, so their widths are unverified. (all)
- `settings.listing.spaceCalculatesFolderSize.*`, `fileExplorer.folderSizes.notConnected`: no screenshot of the switch
  or the Calculate-folder-sizes toast. (de…zh-Hant)
- `fileExplorer.quickFilter.*`, `settings.fileExplorer.typeToJump.mode.*`: no screenshot of the toast, the "Filter: …"
  badge with its ×, or the Jump/Filter toggle, so widths are guesses. Capture all three. (11)
- `commands.handler.getInfo.*`: no screenshot of either toast or the button, so widths are guesses. (11)
- `fileExplorer.navigation.favoriteUnreachable.*`, `favoriteStatus.*`: no screenshot of the toasts or the dimmed-row
  tooltip, so widths are unverified. (pt)
- `fileExplorer.navigation.favoriteStatus.connects`, `favoriteUnreachable.forgotten`: `{place}` is a share, a server, or
  a phone, and nothing says which, so case languages can't put a type noun before it (`к серверу «…»`) and fall back to
  a parenthesis or colon. Pass a `{kind}` select (share / server / phone) alongside it. (ru)
- "Extension" is both `menu.sort.extension` and `multiRename.extensionMask`, so term-consistency forces one word; a
  field label might want the fuller form. Consider "Extension mask" or an allowlist entry. (sv)
- No screenshot of the Multi-Rename sheet, so label lengths (counter row, title-case option) are unverified. (all)
- `multiRename.presets.*`: no screenshot of the Presets menu, submenu, or name popover, so lengths are unverified. (all)
- `multiRename.searchOptionHelp.on` / `.off` share "On"/"Off" with the settings switch options, so `term-consistency`
  forces the settings form, while in gendered languages the tooltip row agrees with "option" (pt wanted `Desativada`,
  shipped `Desativado` to pass). Add an `agreesWith`-style hook or an allowlisted reason so a locale can agree with the
  option. (pt, ru)
- Tooling: the brief's reference-pile path (`<main clone>/_ignored/i18n/<tag>/`) doesn't exist on the M1 agent box, so
  "no ruling" terms there can't be mined as the brief requires. Sync the pile to the box, or have the brief say so and
  fall back to `terms.json` sources; vi mined the live Finder bundle instead (§ No pile on this machine). (ru, vi, sv)
- `servers.sheet.addressMoveHelp`, `s3EndpointMoveHelp`, `servers.refusal.addressTaken`: no screenshot of the edit sheet
  in its move state; one would confirm the line sits under the Address / Endpoint field.
- `fileOperations.sourcesGone.*`: "this delete" / "A queued delete" use delete as a noun, and "since you queued it" has
  no `queued` concept. Locales rendered it as "while it waited its turn", matching the queue's Waiting status.
- `transfer.metadataLeftBehind`: says "This server" though the description lists phones and cloud buckets too; "those"
  points back at the `{classes}` insert. Name the destination generically and use a noun ("that information").
- `transfer.metadataClass.*` joined by `formatDisjunctionList`: "or" inside a negation reads stiff in fr (wants `ni`),
  hu, and nl; a conjunction ("and") or a colon-list frame would read naturally everywhere. (fr, hu, nl)
- Term consistency ties `fileOperations.transferTree.state.compressing` (a row's state) to
  `fileOperations.transferProgress.stageCompressing` (the dialog's stage); fr and nl wanted different forms. (fr, nl)
- `queue.row.itemCount`, `.folderCount`, `.fileCount`: the row splices them after the `queue.row.label` action word
  ("Copying 3 items"), but a case language wants the genitive after a verbal noun (ru `Копирование 3 объектов`) and the
  count also stands alone in the tooltip, so one form can't fit both. Consider a whole-row key with the counts inside,
  or say in the descriptions that the chunk reads as a separate column (ru ships nominative for now). (ru)
- Term consistency ties the badge `fileExplorer.fileIcon.sync.downloading` ("Downloading") to the progress line
  `settings.mediaIndex.clip.downloading` ("Downloading…"), since edge punctuation is stripped. Finder's own badge reads
  differently (de `Laden …`, `AXBADGE8`), so a locale can't follow Finder there without an allowlist entry. Decide
  whether the badge states split from that group or stay tied. (de)
- The reference pile has no git vocabulary (detached, unborn, ahead/behind, uncommitted changes): no tier names them.
  git's own `po/<lang>.po` (github.com/git/git, `po/`) is the authoritative translation for ~20 languages; propose
  adding it to the pile as a git-domain source. sv mined it live for `git-detached`, `git-unborn`, `git-dirty`. (sv, ru,
  es, de)
- `concepts.json` `archive-chat` matches `fileExplorer.list.rowKind.archive` ("archive", a zip file), so the brief lists
  the chat-archiving verb for it. Add that key to `archive-chat`'s `notMatch` and let the `archive` concept match it; hu
  carries a per-locale `exceptions` entry meanwhile. (hu, nl, vi, pt, sv, es, ru, zh-Hant, de)
- Term consistency ties `fileExplorer.list.parentRowName` ("Parent folder", a spoken noun naming the `..` row) to
  `menu.go.parentFolder` ("Parent folder", the Go command, which follows Finder's Enclosing Folder: ru
  `На уровень выше`). The description asks for a noun, so the row reads `Родительская папка`. Reword one English source,
  or add a term-consistency allowlist entry per locale (ru needs one now). (ru)
- Term consistency ties `fileOperations.transferProgress.sizeProgressValueText` (spoken by a screen reader) to
  `viewer.pull.progress` (a visible line), both "{doneText} of {totalText}". A compact slash suits the visible line but
  reads badly aloud. hu realigned both to its shipped `{free} szabad, összesen {total}` frame; vi, zh, and zh-Hant now
  diverge. Reword one English source, or allow the split per locale. (hu)
- `concepts.json` `image-search` groups photo, semantic, by-description, keyword, and tag search under one term, though
  each has its own UI name, so the term check flags every locale. Split it or give each feature its own match. (zh-Hant)
- `decisions.md` is at its byte budget for es and zh, so the #457 rulings (zh genuine `真正的`, guest session
  `客人会话`) went to `review-queue.md` instead. Bump `decisionsBytes` in `i18n-termbase-baseline.json` if they belong
  there. (es, zh)
- `errors.write.sourcesSpanVolumes.title` / `.suggestion`: "a view" names a read-only virtual folder (backup snapshots,
  archive inside, git history), but the word is the View menu / F3 Viewer elsewhere and no UI surface calls these
  "views". A literal rendering (hu `nézet`) reads as the display mode. Consider wording the users see, like "inside a
  backup, archive, or repository history”, or add a `virtual-view` concept with that sense. (hu, de, zh, ru: used
  `виртуальная папка`, the catalog's word for the git portal's folders; sv used `vy`, same risk; vi used
  `khu vực chỉ để xem`, "a view-only area", since `chế độ xem` is the display mode; pt used `visualização`, same risk;
  fr used `vue`, same risk; zh-Hant used `虛擬資料夾`, since `檢視` is the verb and `顯示方式` the view mode; nl used
  `weergave`, same risk; es used `vista`, same risk, since it's also `la vista breve` / `la vista completa`)
- `fileExplorer.restic.forgetPassword.noBackup`: the description says to keep “Browse snapshots” in English; confirm
  that folder's name is never localized, or the quoted label goes stale the day it is. (all)
- `fileOperations.transferProgress.conflictPolicyCaption`: `{policy}` reuses the copy dialog's BUTTON labels, which most
  locales write as imperatives (nl `Overschrijf indien ouder`, `Vraag voor elk`), so the caption reads “If a file
  already exists: Overwrite if older!” as a command. Consider dedicated noun-phrase or state labels for the caption
  (“Skip”, “Overwrite if older”) as `*PolicyName` keys. (all with an imperative/infinitive split: nl, de, hu, sv, …)
