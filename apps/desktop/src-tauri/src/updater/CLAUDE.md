# Updater module

Custom macOS updater that syncs files *into* the existing `.app` bundle, keeping its `com.apple.macl` xattr and giving
each per-file write the fresh inode the code-signing cache needs. macOS-only; other platforms use the Tauri updater
plugin from the frontend.

## File map

- `mod.rs`: the four Tauri commands (`check_for_update`, `update_write_blocker`, `download_update`, `install_update`),
  shared `UpdateState` (what the last check offered, what the last download staged), and the typed outcome and errors.
  Tests in `tests.rs`.
- `bundle_location.rs`: whether the running bundle can be written into at all (`BundleWriteBlocker`).
- `manifest.rs`: parses `latest.json`, compares versions, resolves the platform key.
- `signature.rs`: minisign verification against the main and backup updater keys.
- The Apple code-signature check and the tarball extraction live in `crates/cmdr-update-trust/CLAUDE.md`, shared with
  the release gate.
- `installer.rs`: tarball extraction, sync into the running bundle, privilege escalation. `running_bundle()` is
  crate-visible for `crate::install_location` (the Dock pin and the reveal handler).
- `advisories.rs`: the `SecurityNotice` an install answers, from the staged bundle's list.

## Must-knows

- **Sync into the bundle, never replace the `.app` directory**, because the per-file atomic rename below needs a bundle
  to sync into. ❌ NOT because replacing loses the FDA grant: it doesn't (measured; `DETAILS.md`).
- **Per-file writes use atomic rename (temp + `rename()`), not in-place `fs::copy`.** The kernel code-signing cache keys
  on inode, so a same-inode copy dies with `SIGKILL (Code Signature Invalid)` on launch. `rsync -a` (the admin path)
  already renames.
- **Staging dir is per-instance** (`installer::staging_dir`, keyed on `CMDR_INSTANCE_ID`). A shared one makes
  concurrent `Cmdr` processes (main + a worktree) race and trip `ENOTEMPTY`.
- **Only a real user's production install may check** (`skip_reason`). ❌ Never keep a second copy of
  `crate::prod_instance::NON_PROD_ENV_VARS` here. `DETAILS.md` § Who may check.
- **A read-only bundle is EROFS, not EPERM, and no amount of admin fixes it.** App Translocation and a mounted `.dmg`
  refuse root too. `installer::install` and the frontend both gate on `bundle_location::classify` BEFORE the download,
  ❌ never by escalating (an auth dialog the user can only cancel). `DETAILS.md` § A bundle that can't be written.
- **Minisign (main or offline backup key) on the archive, AND Cmdr's Developer ID on the bundle.**
  `vet_staged_bundle` refuses a bundle not code-signed by team `83H6YAQMNP` (`UntrustedCodeSignature`) before reading
  it; ❌ nothing may skip that, tests inject it. The backup pubkey lives only in `updater-backup.pub`, also read by
  `scripts/release-finish`; ❌ never copy it. `DETAILS.md` § Code signature, § Two updater keys.
- **The signature doesn't name a version, so the archive's own `Info.plist` does.** Its trusted comment is only
  `file:Cmdr.app.tar.gz`, and `latest.json` isn't signed, so `installer::refuse_unless_newer` refuses a staged bundle
  whose `CFBundleShortVersionString` isn't newer than the running build. ❌ Never install around it: it's what stops an
  older signed release from rolling an install back.
- **The organization's policy gates every step, and the backend decides.** `check_for_update(trigger)` answers a typed
  `UpdateCheckOutcome` (`DisableUpdates` and a refused background check return before any request); `download_update`
  takes no URL and fetches only what the last check offered; `install_update` re-asks
  `ManagedPolicy::update_to` on a fresh read, then again for the version the extracted `Info.plist` names. ❌ Never let
  the frontend name a URL or version. `DETAILS.md` § Managed policy.
- **Security notices come only from the vetted staged bundle, ❌ never `latest.json`** (unsigned). Read after every
  vet check passes; a broken list ❌ never blocks the install. `DETAILS.md` § Security notices.
- **Manifest fetch is bounded** (10 s connect, 30 s overall); download and install are untimed. The check and the
  download ride `server_request::send` (`Egress::UpdateCheck`, `UpdateDownload`).

Sync order, deletion pass, signatures, privilege escalation, who may check, and the managed policy: `DETAILS.md`. Read
it before any non-trivial work here.
