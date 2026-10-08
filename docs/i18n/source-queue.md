# Source queue

Translators log English-side problems here while translating: ambiguous or inconsistent English, a weak `@key`
description, a missing screenshot, or a rule every language should follow. One bullet per item, at most two lines: the
key(s), what's wrong, the suggested fix. The lead fixes the English, the description, or the shared docs (promoting a
rule into `translation-principles.md` or `translator-instructions.md`), then deletes the entry; git keeps the history.

## Open

- `fileOperations.mkdir.notCreatedToast`, `.mkfile.notCreatedToast`, `fileExplorer.clipboard.notPasted`: "Cmdr couldn’t
  X" makes `dont-translate` demand Cmdr, so cs can’t use its house form „X se nepodařilo“ and must write the gendered
  „Cmdr nedokázal“. Drop the brand from the English, or let the check accept an impersonal restructuring.
- `fileExplorer.network.browser.removeHostConfirm`, `.hostRemoved`, `.hostRemoveFailed`: descriptions (and key names)
  still say "remove" while the English says "Forget". Say forget, and that the host is a saved server.
- `fileExplorer.network.share.useGuest`: "Use guest" is terse and the description doesn't say whether it reconnects as
  guest or re-lists the shares. Consider "Switch to guest" or "Browse as guest".
- `menu.network.unpin`, `servers.pinHint.body`: the descriptions still call it a "very short label" / quote "Unpin", but
  the English is now "Unpin from switcher". Update both; "switcher" alone doesn't hit the `volume-switcher` concept
  either.
- `servers.paneState.notConnected`: "{name} isn't connected" forces agreement with `{name}` in gendered languages, and
  the description says "yet" where the English doesn't. Prefer "Not connected: {name}"-style framing.
- `servers.hub.shareAccount`, `.guestAccount`: "as {username}" has no bare equivalent in many languages; say a label
  form ("Account: sven") or an added verb is fine, and add a screenshot.
- `fileExplorer.navigation.forgetConfirmButton`: one button serves the server, share, and saved-password alerts; locales
  that clear a password with another verb (zh `清除`) get a button that doesn't match the title. Consider a separate
  key.
- `servers.hub.editPickHint`: "Select a server" pulls locales toward their file-marking verb; the English means moving
  the cursor to a row.
- Cross-language rule proposal: when English prose names a button without quotes (`servers.sheet.addAnywayHelp`,
  `servers.pinHint.body`), a locale may quote it; or quote it in English too.
- `list` concept: matches the verb "lists" (Cmdr lists …); add a verb-sense `notMatch` so locales stop needing
  exceptions.
- `go-back` concept: matches "forward" while its headword is "Go back"; split or rename.
- `servers.refusal.s3FieldMalformed`: "lowercase letters" means Latin a–z, but a Cyrillic or accented letter is
  lowercase too. Say "Latin letters" in the English or the description (ru added `латинские`).
- `fileExplorer.archivedFile.label`, `askCmdr.sessions.archivedBadge`: both are "Archived" in English, so every locale
  needs a term-consistency allowlist entry. Consider "In cold storage" for the file glyph.
- `fileExplorer.network.share.signIn{Title,Message}`, `fileExplorer.networkMount.signIn{Title,Message}`: the screenshot
  is the servers list, not the calm sign-in screen these strings sit on. Capture that pane (with and without the sheet).
- `fileOperations.transferProgress.stage*` (compress and archive-upload phases, and their `*Step` variants): coupled to
  `transfer-dialog.png`, whose note lists only scanning/paused/queued/finishing. Capture a compress-to-remote run
  showing the "Step 1 of 2" line, and name the compress phases in the note.
- `step` concept: its definition says onboarding, setup guide, or install; widen it to any numbered stage of a
  multi-step operation (the two-step compress-then-upload labels now use it).
- `zip` has no concept, yet it recurs in prose (`errors.write.archiveEntryName*`, `settings.archives.*`). Register it so
  each locale's prose form is ruled (de neuter `Zip`, sv `zip-fil`, hu `zip archívum`, zh-Hant `zip`).
- `errors.write.archiveEntryName*` messages: "some tools", "zip tools", and the verb "share" tripped `ai-tool` and
  `network-share`; `notMatch` entries now cover these, but a sense-aware matcher would stop the next one.
- `servers.hub.nearbyGroup`: no screenshot, and the description doesn't say whether a count-last shape is fine (hu, zh,
  and zh-Hant lead with "found nearby"). Recapture the hub with the group header showing, and say the order is free.
- `errors.write.destinationNotAFolder.message` leaves `{path}` bare while its one-line twin
  `errors.volume.notADirectory` quotes it (“{path}”). If the dialog styles the path itself, say so in the description;
  otherwise quote it in both.
- `pnpm i18n:brief --keys a b c` (space-separated) silently briefs only `a` (header says "1 key"). Reject stray
  positional args, or accept both separators.
- Tooling: `sync-locale-keys.ts --restamp` skips overlays (`es-419`), so a new overlay fork's `sourceHash` (the hash of
  the `es` value it overrides) had to be computed by hand with `sourceHash()` for
  `fileExplorer.pane.openLocalNetworkSettings`. Let `--restamp` (or a `--fork`) stamp overlay keys too.
- `{localNetwork}` keys (`fileExplorer.pane.directConnectionBlockedByThisMacToast`, `.openLocalNetworkSettings`,
  `servers.refusal.localNetworkHint`): no screenshot of the toast or the Add server hint. Capture both so translators
  can judge the button's width and whether the pane name reads better quoted (zh and zh-Hant quote it, the rest don't).
- `volume-switcher` matches only "volume switcher" / "volume chooser", so a bare "switcher"
  (`settings.behavior.serversPinHintSeen.label`, `menu.network.unpin`) shows as "No concept yet". Add `switcher` to its
  `match`, with `app switcher` in `notMatch`.
- `settings.adb.install.intro`, `settings.fileOperations.adbEnabled.description`: the descriptions say to use Google's
  localized name for the platform tools and never keep the English, but Google doesn't localize "SDK Platform Tools" in
  most locales (sv, nl, fr, de checked). Say: keep the English where Google does. Also say whether "choose Re-check"
  means clicking (sv, nl, and es wanted `klicka på` / `klik op` / `haz clic en`). (sv, nl, fr, de, ru)
- Go to folder: Finder pt-BR splits its menu and dialog wording, which `i18n-term-consistency` forbids; the `goToPath.*`
  `@key` notes should say the menu item wins. (pt)
- `errors.eject.unmountRefusedByProcesses`: zh-Hant's list join can run Latin into Han with no space
  (`cfprefsd和其他程序`). (zh-Hant)
- `servers.paneState.unreachable`, the eject-process keys: say in the descriptions which shipped sibling to mirror
  (`servers.refusal.unreachable`, `errors.eject.otherApps`); every locale converged on them anyway. (fr)
- `adb.disconnectBusyTooltip`: "Disconnect" doesn't say whether Cmdr drops the device or the person leaves it, which
  decides transitive vs reflexive in fr. (fr)
- No concept yet for "called", "others", "reach", "usual", "leaving", "android", "adb"; "switcher" is the one that
  matters (zh-Hant has both 卷宗切換器 and a bare 切換器). (de, zh-Hant)
- `fileExplorer.listingStalled.*`, `indexing.overall.*`: no `screenshot`. Capture the stalled-listing pane and the
  checklist with the whole-run line, so locales can judge length and the spinner context. (all)
- `indexing.overall.eta`: `{eta}` can be "Almost done", which arrives capitalized after the colon; several locales then
  read "Total: Almost done". Say in the description whether the inserted phrase is sentence-initial or not. (de, hu, vi)
- `errors.write.insufficientSpace.*`, `fileOperations.errorDialog.copyAnyway`: no screenshot of the dialog with "Copy
  anyway". (all)
- `downloads.fda.message`: the description requires keeping Full Disk Access in English. Instead require the localized
  macOS permission label, as other FDA keys do.
- Plural instruction proposal: CLDR `one` does not mean exactly one. Audit counts such as 21/101; use `=1` for one-only
  wording and keep the displayed count in the ordinary `one` branch.
- `fileExplorer.tabBar.paneTabsAriaLabel`: `{paneId}` arrives as raw `left`/`right`, so screen readers say "панели
  left". Use a `select` in the English, like `fileExplorer.pane.filePaneAriaLabel`. (ru)
- `askCmdr.event.chatMemoryChanged`: `{tokens, number}` plus a fixed "tokens" can't agree in count-agreement languages
  (`16 384 токенов`). Make it a `{tokens, plural, …}` in the source. (ru)
- `queue.chip.ariaLabel`: the description wants "percent" as a word, which can't agree with a preformatted
  `{percentText}`. Allow `%`, or pass a numeric `percent` for an ICU plural. (ru)
- `queryUi.age.*`: `count` is passed as a string (`recent-items-utils.ts`), so count-dependent abbreviations (ru `г.` /
  `л.`) can't pluralize. Pass the number too. (ru)
- `queryUi.date.preset.thisMonth`, `.lastMonth`: `{month}` arrives nominative, so Slavic locales can't say "from the 1st
  of October" (`с 1 октября`). Pass a preformatted day-and-month. (ru)
- `errors.eject.*`: the host "Couldn't eject {volumeName}: …" puts a capitalized standalone sentence after a colon;
  Russian wants lowercase there. Say whether the fragment may start lowercase. (ru)
- `errors.eject.busy`: the English says "moving files there", but the description says copy, move, or delete; widen the
  English ("still working with files there"). (ru)
- `errors.write.readOnlyDevice.source.fallbackName`, `.destination.fallbackName`: the descriptions say "the subject of
  'is read-only'", which pushes gendered frames; say the value can be an archive or a `.git` history. (ru)
- `errors.listing.notSupportedErrno.suggestion` and other literal sizes: descriptions say keep `4 GB` Latin, while
  Russian macOS writes `4 ГБ`. Decide whether unit symbols localize, in the formatter and prose together. (ru)
- `settings.behavior.openTerminalHereApp.label`: "a label continued by the dropdown value" breaks in case languages
  (`… в Как в системе (…)`). Say a noun label is fine, as `settings.behavior.textEditorApp.label` does. (ru)
- `indexing.step.findFilesPhased`: the description says no trailing period on the second sentence, but the English has
  one. Align them. (ru)
- `fileOperations.transferDialog.rootEchoWarning`: "This place already starts in {rootFolder}" is hard to parse. Say
  "This location's path already starts with {rootFolder}…". (ru)
- `fileOperations.cancelRollback.reason.unverifiable.named`, `askCmdr.renameUndo.skipReason.unverifiable.named`:
  byte-identical English, but one is about items and the other about files; split the wording or say so. (ru)
- `menu.bar.select`: the description asks for an imperative verb, while `menu.context.selection` asks for its noun.
  Two-pane managers title this menu with a noun (TC "Mark", ru `Выделение`); allow either. (ru)
- `servers.paneState.cancelCycleTooltip`: "Switch back to retry" doesn't say switch back to what. (ru)
- `menu.volume.editFavoriteShortcut`: "Set shortcut…" captures one A–Z key, so locales write "key combination". Consider
  "Set key…". (ru)
- `servers.paneState.retryKeepsTrying`: say `{duration}` is the whole cycle, not the remaining time, so nobody writes
  "another N minutes". (ru)
- `crashReporter.sentToast.changeSettings`: "Settings > Updates" names a section that's now "Updates & privacy"
  (`settings.section.updatesAndPrivacy`). (ru)
- `common.attachEmailPlaceholder`, `settings.updates.emailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`:
  "localize the local part to your word for you" fails in Cyrillic scripts (`вы@example.com` isn't typeable). Allow a
  generic Latin `name@`. (ru)
- `pnpm i18n:brief --changed-since <ref>` diffs against the ref's tip, so a branch's brief also carries every key `main`
  changed after the split (about 11 per S3 brief). Compare against `git merge-base <ref> HEAD` instead. (all)
- S3 keys (`servers.sheet.s3*`, `servers.refusal.*` S3 arms, `*shareLink*`, `*coldStorage*`, `fileOperations.s3Cost.*`):
  none has a `screenshot`. Capture the S3 form, a refusal, the share-link submenu, and the cost line. (all)
- `errors.write.invalidName.suggestion` changed English (tabs and line breaks) in the same commit as the S3 batch but
  wasn't in its `--keys` brief; the stale check caught it. Brief stale keys alongside new ones. (all)
- `remove` concept: deleting a saved credential (`ai.secretError.removeTitle`, `onboarding.cloudSetup.removeKey`,
  `fileExplorer.network.share.forgetPasswordTooltip`) is a real deletion; consider a `notMatch` so vi stops needing
  exceptions. (vi)
- `network-share` concept: matches the verb in "share it yourself" (`errorReporter.dialog.managedOff`); add a verb-sense
  `notMatch` so locales drop their per-locale exceptions. (de, es, hu, nl, pt, ru, sv, vi, zh, zh-Hant)
- `organization` concept: its sense says only "the company a commercial license is issued to"; widen it to the employer
  or school that manages the Mac through MDM (`settings.managed.*`, `ai.managed.*`). (fr, hu, vi, zh, zh-Hant)
- `settings.managed.summary.upTo`, `.upToManualChecksOnly`, `updates.status.heldByPolicy`: a bare `{ceiling}` forces
  some locales to add "version", and "Up to" doesn't say it's inclusive. Say "Up to version {ceiling}" or "inclusive" in
  the description. (fr, nl)
- `errors.serverRequest.blockedByPolicy`: "turned this off" has no noun to translate; name it ("this feature") or say in
  the description what "this" refers to. (fr, ru)
- `ai.managed.cloudAiOff`, `.localOnlyUnsupported`, `settings.managed.summary.onDeviceOnly`: say "on-device AI" while
  the provider option is "Local"; use one English name, or say in the descriptions both mean the same model, and relate
  the `on-device` concept to it. (vi, zh, nl)
- "IT team" (`ai.translateError.managed.body`, `ai.managed.hostNotAllowed`, `askCmdr.error.managedByOrganization`) and
  MDM "manages" have no concepts, so locales split (team, department, staff). Register `it-team` and
  `managed-by-organization`. (ru, zh-Hant)
- `settings.managed.*`, `ai.managed.*`, `askCmdr.error.managedByOrganization`, `onboarding.stepBeta.analyticsManaged`:
  no screenshot of the managed card or the locked rows, so value widths are guesses. Capture them under a policy. (ru)
- `fileExplorer.compareDirectories.*`: no screenshot of the compare toasts, so their widths are unverified. (all)
- `settings.listing.spaceCalculatesFolderSize.*`, `fileExplorer.folderSizes.notConnected`: no screenshot of the switch
  or the Calculate-folder-sizes toast. (de…zh-Hant)
- `fileExplorer.quickFilter.*`, `settings.fileExplorer.typeToJump.mode.*`: no screenshot of the toast, the "Filter: …"
  badge with its ×, or the Jump/Filter toggle, so widths are guesses. Capture all three. (11)
- `settings.fileExplorer.typeToJump.mode.description`: the description says to use "the names printed on a Mac
  keyboard", but most non-US Mac keyboards print only the ⌫ glyph and an English "esc", so there's no printed name to
  copy. Say "the name macOS gives the key in your language (VoiceOver's key names)" instead; that's what the
  `delete-key` rulings record. (11)
- `commands.handler.getInfo.automationOff`, `.openAutomationSettings`: hardcode "System Settings > Privacy & Security >
  Automation" while siblings use the runtime `{system_settings}` / `{privacy_and_security}` / `{localNetwork}` tokens
  read from the user's Mac; use tokens (add an `{automation}` one) so a pane rename can't drift. (11)
- `commands.handler.getInfo.*`: no screenshot of either toast or the button, so widths are guesses. (11)
- Brief proposal: `pnpm i18n:brief --lang all` leaves out the `es-419` overlay, yet `i18n-es-overlay.test.ts` fails
  until new `es` keys using `Ajustes` or the compound perfect get forks. Have the brief list the overlay forks a batch
  needs. (es-419)
- `settings.appearance.tintMtp.*`: the notes say keep `ADB` and `Kindle` verbatim, but neither is in `BRAND_WORDS`, so
  `dont-translate` can't guard them. Add both (`ADB` alongside `MTP`). (11)
- `multiRename.counterStart` "Counter from" is a fragment, and `.counterStep` / `.counterDigits` don't name the counter
  (no group box like TC's). Consider "Counter start", "Counter step", "Counter digits". (fr, ru, vi)
- `multiRename.case.lower`, `.upper`, `.words`: say whether locales mirror the show-the-result casing. Proposed rule:
  such keys are exempt from sentence case, and scripts without case (zh, ja, ko) describe the transform. (fr, sv, vi,
  zh-Hant)
- `multiRename.preset.default` and `.status.unchanged` share "No change" across two roles (a preset name, a row status),
  forcing identical translations. Give the preset its own English ("Keep names"). (zh, zh-Hant)
- `multiRename.removeDiacritics` and `.preset.removeDiacritics` share one English for a checkbox and a preset name; and
  say whether an everyday "accents" word is fine, since ß→ss goes beyond diacritics. (ru, pt, fr, nl)
- `onboarding.stepAi.table.renameWithout` ("batch rename UI") and `.rowRename` ("Mass-rename") likely name the new tool;
  align the English to "Multi-rename". (de, hu, es realigned their values)
- `multiRename.search` "Search for" is find-and-replace, yet the `search` concept means finding files; zh needed
  exceptions (查找 vs 搜索). Add a `find` concept or a `notMatch`. (zh)
- "Extension" is both `menu.sort.extension` and `multiRename.extensionMask`, so term-consistency forces one word; a
  field label might want the fuller form. Consider "Extension mask" or an allowlist entry. (sv)
- No screenshot of the Multi-Rename sheet, so label lengths (counter row, title-case option) are unverified. (all)
- Tooling: the brief tells translators to restamp and edit `terms.json`/`source-queue.md`, which races when 11 run in
  parallel. Consider a `--parallel` brief mode that routes termbase writes to a proposals file. (nl)

### From the Czech (cs) translation pass (2026-10-08)

- `zoom` concept: its match catches `menu.window.zoom` (Window > Zoom resizes the window); Apple localizes it
  differently from text zoom (cs Přepnout velikost). Add a `notMatch` or a separate concept instead of per-locale
  exceptions. (cs seed)
- `unplug` concept: its match includes "disconnected", overlapping the deliberate `disconnect` concept; many languages
  use different words for physical unplugging, so drift hits may be wrong. (cs seed)
- Reference pile: the cs macOS pile lacks the Privacy & Security extension's row labels (Full Disk Access, Local
  Network); only search terms are present. The SecurityPrivacy appex harvest may be missing in the extractor (affects
  every language). (cs seed)
- Tooling: `i18n:check-termbase` writes `decisionsBytes` into the tracked baseline on a locale's first run, which
  surprises a worker told to touch only docs. (cs seed)
- `settings.appearance.tintTriggerAria`: the {label} example in the description says "Tint SMB/network panes", but the
  real label is now "Tint server panes (SMB, SFTP, WebDAV, S3)"; update the example
- `settings.appearance.dateTimeFormat.optDesc.short`: the example 01/25 is US month/day; if the Short format is not
  locale-aware, Czech users (day.month) will find it odd; say in the description whether the format follows the locale
- `settings.fileOperations.adbBinaryPath.description`: "Point it somewhere else" has an unclear "it" (the field);
  suggest "Set this to another path if…"
- `settings.listing.sizeUnit.description`: mentions "Fixed units" but the batch only has opt.dynamic and opt.bytes; the
  description should say where the fixed-unit option labels come from (common.sizeUnit.*?)
- `menu.help.whatsNew`: description says the apostrophe is plain ASCII, but the English value uses U+2019 (What’s new).
  Same for `menu.mediaIndex.excludeFolder` (Don’t).
- `menu.sort.name`: description says it doubles as a column header, but the key is only used in the native View > Sort
  by menu (native_strings.gen.rs); the claim pushes translators away from Finder's genitive Sort By children. Same for
  `menu.sort.extension` and `menu.sort.size`.
- `menu.app.checkForUpdates`: the term-drift check (termbase) reports it under the check concept even though the
  concept's description excludes Check for updates; the matcher should honor that exclusion. Likewise the drift matcher
  compares accept stems with a plain space, so every locale that writes a no-break space after a short word (cs, fr)
  misses stems like o chyb; normalize U+00A0 to a space before matching.
- `commands.navUp.label`, `commands.navDown.label`: Select previous/next file only moves the cursor, but select is
  Cmdr’s marking concept; suggest Go to previous file / Go to next file (or Move cursor up/down)
- `commands.*.description`: palette descriptions mix the imperative (Check whether…, Open a window…) with the 3rd person
  (Shows the size…, Renames the selected files…); pick one mood for the family
- `commands.paneCopyPathLeftToRight.label`, `commands.paneCopyPathRightToLeft.label`: Copy path reads like the clipboard
  Copy path (menu.edit.copyPath) though nothing is copied; suggest Open left pane’s folder on the right
- `commands.cursorScrollTo.label`: Scroll pane to index uses the jargon index; suggest Scroll pane to row
- `commands.viewZoomSet75.label` (and 100/125/150): the note says keep 75% as-is, but locales that space a noun percent
  (cs 75 %) need to; say keep the number, percent spacing per locale
- `commands.fileGoToTrash.description`: the note says the apostrophe is doubled for ICU, but the English value uses the
  typographic ’ (nothing to double); the note is stale
- `commands.helpWhatsNew.label`, `commands.viewCalculateFolderSizes.label`, `commands.cmdrOpenOnboarding.label`: the
  notes should say the command and its menu item (menu.help.whatsNew, menu.view.calculateFolderSizes,
  menu.app.onboarding) must read the same, as commands.queueShow.label does, so parallel translators align
- `errors.eject.unmountRefusedByProcesses`: a raw (non-ICU) string carries {countText} before a counted noun, so
  languages with count-dependent noun forms (cs, pl, ru, uk…) can't agree. Make it ICU with a {count} plural, or drop
  the count.
- `errors.listing.*Errno.*` and their non-errno twins (`errors.listing.notFound.*`, `errors.listing.permissionDenied.*`,
  `errors.listing.alreadyExists.*`, `errors.listing.cancelled.*`, `errors.listing.storageFull.*`) share identical
  English across translation batches; the batch split separated them, so twins land in different batches. Keep twins in
  one batch.
- `commands.serversTogglePin.label`: Pin / unpin server (note: the slash stays) while its sibling
  `commands.tabTogglePin.label` says Toggle pin tab; pick one pattern for the two pin toggles (cs uses Připnout nebo
  odepnout for both)
- `commands.handler.favoriteAdded`: the note says the folder name is in straight double quotes, but the English uses
  curly “…”; the note is stale
- `commands.fileCopyShareLink.label`, `commands.fileCopyShareLinkOneDay.label`,
  `commands.fileCopyShareLinkOneHour.label`: (seven days) is a bare duration, while the context submenu says Expires in
  seven days (`menu.context.shareLinkSevenDays`); consider (expires in seven days) so the two surfaces match
- `commands.handler.viewDebugLog.disabled`: Log storage and disk space limit trip the storage and free-space concepts;
  add them to those concepts' notMatch
- `commands.cloudAskGemini.description`: with the selected file as its subject is vague; suggest Opens Google Drive’s
  Gemini assistant to ask about the selected file
- `fileExplorer.archivedFile.label`: the concept matcher tagged it archive-chat (Ask Cmdr chat archiving), but it is the
  cold-storage sense; add it to archive-chat’s notMatch
- `fileExplorer.network.browser.status.loggedIn` / `fileExplorer.network.browser.status.loggedInOk`: identical English
  Logged in for two different states (credentials stored but not yet used vs connected with them); consider Credentials
  saved for the first
- `fileExplorer.errorPane.retryInfo`: Retry #{count} is ambiguous between the ordinal of the current attempt and the
  number of retries so far; say which in the @key description
- `fileExplorer.pane.openLocalNetworkSettings`: {localNetwork} is injected in the Mac’s system language, so case
  languages can’t decline it; a colon or quoted form in English (Open “{localNetwork}” settings) would signal it is a
  name
- `fileExplorer.pane.fileExplorerAriaLabel`: File explorer is the Windows app name; File manager or Files would be more
  neutral
- `fileExplorer.errorPane.goBack`: description asks to keep it short, but the matching command `commands.navBack.label`
  is longer in some locales; note whether the two must match
- `settings.mediaIndex.networkVolumes.alwaysAria`: the aria label (Always index photos on {name}) doesn't contain its
  switch's visible label (`settings.mediaIndex.networkVolumes.alwaysLabel`, Always index this drive), which breaks the
  WCAG 2.5.3 containment every locale is told to keep. Suggest Always index this drive: {name}, or rewording the visible
  label
- `settings.mediaIndex.showInSearch.label`: the description says Search names Cmdr's Search dialog, but
  `search.dialog.title` isn't translated in most locales yet and the label reads as a generic feature. A note saying
  whether the word should be capitalized as a dialog name would help
- `settings.mediaIndex.clip.deleteFailed`: the just-now concept matches this key's couldn't be removed just now, which
  is not a relative-time label. Suggest adding couldn't be removed just now (or just now as an adverb) to just-now's
  notMatch
- `settings.mediaIndex.progress.coveredOfTotal`, `settings.mediaIndex.progress.coveredDone`,
  `settings.mediaIndex.progress.kept`: the placeholder notes say drives plural, which reads like the word drives
  (disks). Suggest selects the plural form
- `errors.write.deviceDisconnected.title`, `errors.write.deletePending.title`, `errors.write.notConnected.title`,
  `errors.write.sourceInColdStorage.title`: identical English to `errors.listing.deviceDisconnected.title`,
  `errors.listing.deletePending.title`, `errors.listing.notConnected.title`, `errors.listing.coldStorage.title` but
  split into another batch; keep twins in one batch (same issue as batch 06 reported).
- `errors.git.blobTooLarge.suggestion`: „Check out the file from a working tree“ is ambiguous (git checkout vs. just
  open the file in a working tree); suggest „Open the file from a working tree…“ or name `git checkout` explicitly.
- `errors.listing.deletePending.suggestion`: the @key description says „the en-dash → is part of the copy“, but the
  English has an em dash (—) and no arrow; fix the description.
- concept `software` matches „hardware“ and concept `ai-provider` matches any „provider“ (storage providers in
  `errors.listing.objectStoreRefused.*`); consider a separate hardware concept and a `storage-provider` concept, or
  notMatch entries.
- `fileOperations.transferDialog.smbNativeNote`: quotes “Connect directly” but the picker item is
  `fileExplorer.navigation.connectDirectly` „Connect directly for faster access“; quote the full label or shorten the
  label
- `fileOperations.delete.cloudOnlineOnlyHandedBack`: names Delete unquoted while the sibling warnings quote “Delete”;
  align
- `fileOperations.s3Cost.infoText`: 'provider' is matched by the ai-provider concept but means the S3 storage provider;
  add a notMatch or a storage-provider concept
- `fileOperations.transferDialog.policyOverwriteSmaller`: 'Overwrite if smaller' doesn't say which file is smaller
  (description says the existing one); consider 'Overwrite if existing is smaller'
- `fileOperations.delete.foldersPart`, `fileOperations.transferDialog.filesPart`,
  `fileOperations.transferDialog.foldersPart`, `fileOperations.delete.selectedFilesPart`: descriptions should say the
  assembled phrase is the OBJECT of the title verb, so case languages pick the accusative (cross-language rule)
- `fileExplorer.folderSizes.unreadable`: the complete concept's matcher fires on „incomplete“; add „incomplete“ to the
  complete concept's notMatch so every locale doesn't need an exception.
- `fileExplorer.edit.hint`: its sibling commands.handler.openTerminalHere.hint names the section („Pick one in Settings,
  under Navigation & file ops“), this one only says „in Settings“; align the two.
- `settings.network.localNetworkAccessLabel`: "{localNetwork} access" makes inflected languages decline an OS-injected
  token; the description should allow a colon form or the string should carry the token behind a head noun
- `settings.behavior.fileSystemWatching.lowDiskSpaceNotifications.description`: concepts.json start-up matches "startup"
  inside "startup disk"; add "startup disk" to start-up notMatch (cross-language)
- `settings.network.smbConcurrency.description`: concepts.json software matches "hardware", so every locale must explain
  a device word; drop hardware from software.match
- `settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.description`: term-consistency compares accept
  stems with plain spaces, but locales whose mechanics require U+00A0 after short prepositions (cs) can never match a
  stem like „přístup k disku“; normalize U+00A0 to a space before matching (cross-language)
- `errors.provider.cmVolumes.displayName`: never interpolated (`errors.provider.cmVolumes.transient` and
  `errors.provider.cmVolumes.nonTransient` have no {name}), same for `errors.provider.genericCloudStorage.displayName`;
  the descriptions claim mid-sentence use. Drop the keys or say they are unused.
- `errors.provider.veraCrypt.serious`: "repair tools" matches the ai-tool concept; add "repair tools" to ai-tool
  notMatch.
- `errors.provider.pCloudFuse.needsAction`: "re-approve pCloud's system extension" matches the approve concept
  (AI-suggested ops); add "re-approve" to approve notMatch (same in `errors.provider.pCloudFuse.serious`).
- `errors.write.trashRefused.suggestion.notPermitted`: hardcodes Shift+F8 while
  `errors.write.trashNotSupported.suggestion` uses {deletePermanentlyKey}; use the token in both trashRefused
  suggestions (also `errors.write.trashRefused.suggestion.noTrashForVolume`).
- `errors.write.deviceDisconnected.sided.source.copy`: "{done} of {total} files" needs plural agreement in most
  languages but the family is raw; consider passing a preformatted count phrase or moving these keys to ICU (same for
  the other `errors.write.deviceDisconnected.sided.*` and `errors.write.trashRefused.message.*`).
- `fileOperations.transferProgress.queuedToast`: {countText} is a preformatted noun phrase used as the sentence subject;
  case/agreement languages can't build a verb around it. Pass {count} too so the verb can sit in a plural
- `fileOperations.transferProgress.titleCancelling`, `fileOperations.transferProgress.titleRollingBack`: the
  cancel/rollback rulings' own progress forms (Rušení…, Vracení…) fail their accept stems; the termbase check should
  also test the ruling's `forms` values
- `fileOperations.cancelRollback.reason.*` vs `askCmdr.renameUndo.skipReason.*`: byte-identical English across two
  features; term-consistency flags them until both files are translated, consider a shared key
- `fileOperations.transferProgress.rollbackAlreadyLandedTooltip`: the stop concept matches 'stops it' here, forcing the
  stop verb onto what is the Cancel button's effect; fine, but the description could say so
- `fileExplorer.summary.of`, `fileExplorer.summary.and`: SelectionInfo.svelte joins these with plain spaces, so a Czech
  one-letter connector (z, a) cannot take the required no-break space; let the fragment carry its own trailing space or
  assemble the whole line in one ICU message
- `fileExplorer.summary.fileNoun`, `fileExplorer.summary.dirNoun`: the noun's case depends on the connector before it
  (Czech z takes the genitive), so word-level assembly of “3 of 10 files” forces case-language translators into a
  genitive-only noun; one ICU message per summary line would let each language build the phrase
- `fileExplorer.tabBar.paneTabsAriaLabel`: {paneId} receives the raw English left/right; say so in the description (the
  cs value uses select on it) or pass a translated side
- `servers.sheet.signInTitle`: the description says only 'a server', but secretAccessKey's note says the S3 sign-in
  sheet uses it too; say which kinds of {name} arrive (server, S3 account, bucket) so inflecting locales can choose a
  head noun
- `servers.sheet.editTitle`: same as signInTitle: name the kinds of {name} (server, S3 account, S3 place)
- `servers.hub.discoveryOffLink`: described as an imperative VERB phrase but it is a link; say whether it reads as a
  continuation of servers.hub.discoveryOff or standalone
- `servers.pinHint.body`: 'Unpin from switcher' is unquoted while {command} is quoted; quote both UI labels the same way
- `errors.write.filesTooLargeForFilesystem.message.many`, `errors.write.originalsKeptAside.message.many`: raw errors.*
  strings with {count} can't pluralize, which forces Czech (one/few/many/other) into a colon workaround. Suggest making
  them ICU, or rewording the English so the count isn't in an agreeing position.
- `errors.write.readOnlyDevice.destination.message`, `errors.write.readOnlyDevice.source.message`: {deviceName} is
  either a name or a whole fallback phrase („The target device“, „The source“), so languages can't put a head noun
  before it or decline it. Suggest separate named/unnamed keys (as `errors.write.moveNotConfirmed.message.named` /
  `.unnamed` already do).
- `errors.write.sourceNotRemoved.suggestion`: „delete the original yourself“ forces gender in languages where 'yourself'
  agrees (cs sám/sama); cs restructured it to „smažeš ručně“. Worth a note in the @key description: 'yourself' means 'by
  hand'.
- `settings.analytics.email.description`: @key says keep you@example.com as-is, but `settings.updates.emailPlaceholder`
  says localize the local part and keep all email placeholders identical; the description's example should follow the
  same rule (and be listed with the other three)
- `settings.askCmdr.enabled.description`: @key calls the switch "Turn on Ask Cmdr", but its label
  `settings.askCmdr.enabled.label` is just "Ask Cmdr"; align the description
- `ai.cloudConsent.logsNote`: says "LLM call logging" but the setting it points to,
  `settings.advanced.logLlmCalls.label`, reads "Log AI model calls"; quote the label
- `ai.translateError.authFailed.body`: uses an ASCII " - " as a dash; use an en dash or split the sentence
- `ai.local.modelSizeUnknown`: description says "inside parentheses" but the value has none (code adds them); say so
- `ai.cloudConsent.askCmdr.item.sizes`: "dates" doesn't say which dates (modified, created); the description could
- `ai.translateError.*` vs `askCmdr.error.*`: the Settings path separator is ">" in one family and "›" in the other;
  unify
- `askCmdr.event.chatMemoryChanged`: uses {tokens, number} with a fixed plural noun "tokens"; make it an ICU plural so
  languages with count-agreeing nouns can inflect (Czech uses genitive tokenů, correct only because the value is always
  in the thousands)
- `askCmdr.error.noCloudConsent`: writes Settings > AI while `settings.askCmdr.provider.off` writes Settings › AI; pick
  one separator for navigation paths
- `askCmdr.tool.refused`: "That request wasn’t available" is vague; something like "Ask Cmdr can’t do that (it’s
  read-only)" would tell translators and users what happened
- `askCmdr.composer.providerOff`: near-duplicate of `settings.askCmdr.provider.off` but says "in settings" instead of
  "in Settings › AI"; consider aligning
- `servers.paneState.cancelCycleTooltip`: Switch back to retry is ambiguous (switch back to what: this tab, this pane,
  the server?). Say what the user does, e.g. Open it again to retry, and describe it in the @key.
- `servers.paneState.retryTotalSeconds`, `servers.paneState.retryTotalMinutes`: the description should say the phrase is
  the object of a duration sentence (servers.paneState.retryKeepsTrying), so case-marking languages pick the accusative;
  these keys must not be reused as standalone labels.
- `settings.summary.navigationAndFileOps`: the @key description explains "extension", but the English string has no such
  word; drop that sentence or update the summary
- `settings.fileSystemWatching.cardDownloads`, `settings.fileSystemWatching.indexSize`: descriptions locate these under
  Settings > Behavior > File system watching, which isn't a section in settings.section.* (Downloads sits under
  Notifications, the index under Drive indexing); update the paths
- `settings.mcp.checkPort`: description says Settings > Developer > MCP server, but the section lives under AI
  (settings.section.mcpServer); fix the path
- `settings.control.unitItems`: a bare unit word after a variable number can't agree in count-inflecting languages (cs:
  1 položka, 2 položky, 5 položek); make it an ICU plural taking the number
- `settings.servers.trustedHostKeys.approvedPrefix`: the prefix is concatenated with a separate DateLabel, so languages
  can't reorder or attach a preposition; consider a {date} placeholder
- `queryUi.results.live.foldersScanned`: note says 'how many folders it has turned up' while English says 'scanned';
  clarify whether the count is folders read or folders found
- `queryUi.recent.modifiedAfter` / `queryUi.recent.modifiedBefore`: description does not say whether these stand alone
  in the tooltip or follow a 'modified' word; English 'after {after}' alone is ambiguous next to 'size …' facts
- `askCmdr.renameUndo.undo`: same English Undo as `fileOperations.trash.undoAction`/`menu.edit.undo` (Odvolat akci in
  cs), but the description asks for a short inline label; the aria sibling `askCmdr.renameUndo.undoLabel` then has to
  start with the full visible label. Consider noting in the description whether the label must match the Edit-menu Undo.
- `askCmdr.renameUndo.skipReason.*.named`: description says the {name} is a file, yet folders can be renamed too; say
  whether {name} can be a folder so languages with a head noun pick file vs item correctly.
- `askCmdr.wake.needsFullDiskAccess`: asks to match `search.coverage.setUpFullDiskAccess`, which is still untranslated
  in cs; translators of either key need the other's final wording (cs used nastav plný přístup k disku).
- `askCmdr.renameUndo.undoJob`: English Undo all {count} batches is always ≥2 per the description, but the description
  does not say so for {count}; state the minimum so languages can drop the one branch wording.
- `onboarding.stepAi.table.searchWithout`: the description says “after 1st of this month” names a date filter option in
  the app, but no catalog key has that label (queryUi has only after {date}); either point at the real key or say it is
  illustrative.
- `onboarding.stepAi.table.rowSelect`: description says “Short verb” while sibling rows (rowSearch, rowRename) are
  naturally nouns in the table; say whether the row labels should be nouns or verbs as a family.
- `onboarding.wizard.stepProgress`, `onboarding.wizard.stepTooltip`: {total}/{mandatory} pass a bare number, so
  languages whose preposition depends on the number’s pronunciation (cs z/ze) cannot agree; consider a number-free frame
  or `{step}/{total}`.
- `onboarding.stepAi.bannerBody.stuck`: the breadcrumb hardcodes Privacy & Security and Full Disk Access while sibling
  `onboarding.stepFda.openSettingsFailed` receives them as OS-localized {privacyAndSecurity} / {fullDiskAccess}
  placeholders; use the placeholders here too.
- `queryUi.date.preset.thisMonth` / `queryUi.date.preset.lastMonth`: {month} is Intl standalone month (nominative in cs,
  pl, ru, uk…), but '1st of {month}' needs the format/genitive form; pass a preformatted {date} from
  Intl.DateTimeFormat({day:'numeric', month:'long'}) (cs: 1. května) instead of the bare month name
- `queryUi.date.preset.thisWeek` / `queryUi.date.preset.lastWeek`: 'this {weekday}' forces gender agreement with an
  inserted weekday in gendered languages; consider documenting the 'weekday of this week' restructure as acceptable
- `licensing.about.fallbackOrg`: the fallback is spliced into „for {org}“ frames, which forces a case in inflected
  languages; the description should say which frames use it (only `licensing.about.perpetual`,
  `licensing.about.commercial`, `licensing.about.commercialUntil`), or the frames should be split into separate no-org
  variants
- `licensing.commercialReminder.priceInfo`: „keep
  $59 literal“ conflicts with locales that place the currency sign after the amount (cs 59 $); suggest allowing locale
  currency formatting or a {price} placeholder
- `licensing.dialog.pendingHint`: {days} is a bare number in a sentence that needs plural agreement; „always 2 or more“
  is fine for English but fragile for cs/pl/ru (2–4 vs 5+). Suggest an ICU plural
- `licensing.dialog.retryExhausted`: {count} is a bare number with „times“; same plural fragility, suggest ICU plural
  (cs uses {count}krát which happens to be invariant)
- `licensing.commercialReminder.usingPersonal` / `licensing.commercialReminder.askCommercial`: the notes say tier names
  stay capitalized; that is an English convention, sentence-case locales should be told they may lowercase them in prose
- `viewer.statusBar.badge.streamingIndexingTooltip`: „in background“ is missing its article („in the background“), and
  „We’re building“ switches to first-person plural while sibling strings name Cmdr or use impersonal voice
- `viewer.toolbar.encoding.detectedSuffix`: „(Detected)“ is title-cased inside parentheses, against the sentence-case
  rule
- `viewer.search.caseSensitive`: „Case sensitive“ is unhyphenated while `queryUi.scope.toggle.caseSensitive` writes
  „Case-sensitive“; align the English
- `onboarding.stepBeta.openBeta`: the <alpha> badge's own text (ALPHA) is not in the catalog, so translators can't tell
  whether it is localized; say in the description that it renders the literal English word ALPHA, or make it a catalog
  key
- `onboarding.stepOptional.mtp.desc`: “this is politely restored” has no clear antecedent (the suppressed macOS
  process); suggest “Cmdr puts that macOS process back when you quit.”
- `onboarding.stepBeta.terms.consent`: a first-person past-tense consent sentence forces a gendered verb in Czech/Slavic
  languages; note in the description that a neutral present-tense restructuring is acceptable
- `search.walkHandoff.counts`: the description says to use matchText/folderText for grouped digits, but the English
  value uses # and never references those placeholders, so a locale that uses them trips the parity check. Either put
  {matchText}/{folderText} in the English or drop the advice.
- `search.coverage.unnamedDrive`: the fallback phrase is spliced into frames with different case needs (accusative
  'indexed this drive', nominative/locative elsewhere) and also into the server variant
  (search.coverage.uncovered.unavailable), where 'this drive' is wrong for a server. Consider per-frame fallbacks or a
  'this server' variant.
- `indexing.eta.secondsLeft`, `indexing.eta.minutesLeft`, `indexing.eta.hoursLeft`, `indexing.eta.hoursMinutesLeft`: the
  N-left phrases are used both standalone and mid-sentence (eta.ts formatEta) but have one form; verb-first languages
  (cs zbývá…, de noch…) need a standalone/midSentence pair like almostDone has
- `indexing.enrich.progress`: pass and document {total} as a plural selector too; in Czech the noun after „z
  {totalText}“ agrees with the total, not with {done}
- `indexing.step.findFilesPhased`: the description says no trailing period on the second sentence, but the English ends
  with a period; align one of them
- `shortcuts.section.pressEscToClear`: English writes ESC, elsewhere the app writes Esc
  (fileExplorer.quickFilter.intro.escape); unify the key name
- `downloads.shortcutRow.modifiedTooltip`, `downloads.shortcutRow.pressKeys`, `downloads.shortcutRow.resetTooltip`: same
  English as `shortcuts.section.modifiedTooltip`, `shortcuts.section.pressKeys`,
  `shortcuts.section.resetToDefaultTooltip`; the downloads translator must reuse Změněno oproti výchozímu nastavení /
  Stiskni klávesy… / Obnovit výchozí
- `multiRename.status.disallowedCharacter`, `multiRename.error.unknown`, `multiRename.error.badRegex`,
  `multiRename.error.gone`, `multiRename.error.readOnly`, `multiRename.moreRows`, `multiRename.hiddenProblems`,
  `multiRename.error.couldntStart`, `multiRename.error.notConnected`: English uses straight apostrophes (Can't, doesn't,
  isn't, won't) while the rest of the catalog uses ’; align to ’ (the en mechanics check already reports 9 quote-mark
  findings for exactly these keys)
- `errorReporter.dialog.sampleFirstHeading`, `errorReporter.dialog.sampleLastHeading`: for count 1 English renders
  „Sample of first 1 line“; an =1 arm („Sample of the first line“) would read better and tell translators the number may
  be dropped there
- `multiRename.counts`: „{unchanged} unchanged“ has no plural, unlike its two siblings; languages whose adjective agrees
  with the count need a plural select
- `main.oldMacos.body`: description asks for first-person warmth but should say a gender-neutral rewrite is fine where
  the first person forces the speaker’s gender (Czech rád/ráda)
- `errorReporter.dialog.description`: „redacted client-side“ is jargon for a reassuring user message; „before it leaves
  your Mac“ would be plainer in English too (the network-share concept also false-matches „share it yourself“ in
  `errorReporter.dialog.managedOff`: add a notMatch for the verb sense)
- `mtp.ptpcameradDialog.helpText`: the description lists a {key} placeholder, but the English has only the <key>…</key>
  tag with literal Ctrl+C; drop the stray {key} line from the description
- `transfer.split.skipped`: description says "already at the target" means the destination folder; the English itself
  should say "destination" to match the copy/move dialogs (target is the symlink sense in the termbase)
- `crashReporter.sentToast.changeSettings`: the English says Settings > Updates, but the section is named „Updates &
  privacy“ (`settings.section.updatesAndPrivacy`) and Updates is only its card (`settings.updates.card.updates`); say
  which one the breadcrumb names, or quote the section name.
- `queue.chip.ariaLabel`: asks to spell "percent" as a word, but {percentText} is a pre-formatted string, so languages
  where the noun agrees with the number (cs procento/procenta/procent, also pl, ru, uk) cannot inflect it; pass the raw
  number as a plural selector or allow the % sign for those locales.
- `ui.toast.age`: {durationText} is described as a number plus a one-letter unit (2m, 1h); confirm it is formatted per
  locale, otherwise Czech shows English unit letters.
- Settings paths use both `›` (`onboarding.localDownloadFailed.body`, `onboarding.stepBeta.signup.unreachable`,
  `settings.askCmdr.provider.off`) and `>` elsewhere; pick one separator for the English. (cs cleanup)
- `fileExplorer.edit.hint`: says only "Pick one in Settings" while `commands.handler.openTerminalHere.hint` names the
  section; align. (cs cleanup)
