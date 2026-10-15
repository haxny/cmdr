# Update path proof (2026-10-15)

Whether the update path still works end to end after the three changes from the 2026-10-15 incident tabletop:
`de66c5171` (the macOS updater requires Cmdr's Developer ID code signature on the bundle), `00291264b` (the updater and
`release-finish` trust a main OR an offline backup minisign key), and `c9fd205d8` (the release build split into a
secret-less `compile` job and a `sign` job). Proven with real artifacts: a real tag push through the real pipeline, the
real `release-finish` signing path, and a real installed app updating itself on a second Mac.

**Verdict: safe to release.** The new pipeline produces bundles the new updater accepts, both the shipped 0.52.0 updater
and the new one install them, and the new updater refuses every doctored archive with a typed refusal while the old
build stays intact. One blocker for the next release was found and fixed on the way: `main`'s full CI run was red (see §
Findings), and a release can't be tagged over that.

## What was built (commits on `worktree-update-path-proof`)

- **The release gate**: `crates/cmdr-update-trust` now holds the updater's trust check (the code signature, the
  installer's extraction, the bundle version), shared by the app and a `cmdr-vet-update` binary. `release-finish` runs
  it before signing; the pipeline's new `vet-update-archives` job runs it before `publish`, in both signing modes. TDD:
  the crate's archive tests and the Go `vet_test.go` were seen failing first. How it works:
  `crates/cmdr-update-trust/DETAILS.md`, `docs/guides/releasing.md` § The release gate.
- **`release-finish -dry-run` takes a pre-release version** (`0.52.1-pipelinetest.1`), so a pipeline test tag's draft
  can go through the signing path. Real runs still refuse one.

## Step 1: the gate on known archives

- `cmdr-vet-update` on v0.52.0's three published archives: all pass. At a wrong version: refused (`WrongVersion`).
- The opt-in real-release test (`a_published_release_passes_and_its_doctored_copies_dont`) passed on v0.52.0 aarch64 and
  on the pipeline test's aarch64 archive: genuine passes, ad-hoc re-signed is `NotOurs`, a changed `Info.plist` byte is
  `Invalid`.

## Step 2: a real pipeline artifact

Tag `v0.52.1-pipelinetest.1`, signed with David's SSH tag key (`git tag -s`), on a
`chore(release): v0.52.1-pipelinetest.1` commit (version bumped in `package.json`, `tauri.conf.json`, `Cargo.toml`,
`Cargo.lock`) whose parent (`d3ca8a08c`) had a green full CI run and a green CodeQL run, so the `ci-gate` parent rule
was exercised too. `RELEASE_UPDATE_SIGNING` was `local`. Run 38755716207, 2026-10-15:

- `guard` green: the tag signature verified against `.github/release-signers`, and `sort -V` ranked
  `0.52.1-pipelinetest.1` above the live `0.52.0`.
- `ci-gate`, `draft`, `sbom`, three `compile`, three `sign`, `pkg`, `vet-update-archives`, `attest`: all green.
  `publish` and `bump-tap` skipped. Afterwards `getcmdr.com/latest.json` still said `0.52.0` and the release list showed
  only a draft.
- `vet-update-archives` (the new job) passed all three archives: "Cmdr's Developer ID build of 0.52.1-pipelinetest.1".
- A pre-release version bundles, signs, and notarizes fine (Tauri CLI 2.12.1).

Then on the laptop (macOS 27.0), against the draft:

- `./scripts/release-finish.sh -dry-run -out <dir> 0.52.1-pipelinetest.1`: provenance verified for all three
  (`release-pipeline.yml` at `108d401e9`), the gate passed all three, the main key from sops signed all three, and each
  signature verified against the trusted-key list ("key A2601F36BB168C0A"). Nothing uploaded.
- On each extracted app (aarch64, x64, universal): `codesign --verify --deep --strict` valid and satisfies its
  designated requirement; `spctl -a -vv` "accepted, source=Notarized Developer ID, origin=Developer ID Application:
  Rymdskottkarra AB (83H6YAQMNP)"; `xcrun stapler validate` worked; the designated requirement is byte for byte
  `RELEASE_REQUIREMENT`.

## Step 3: the real update on the M1

The M1 MacBook (`m1mbp`, macOS 27.0.1, arm64).

**The seam**: the updater has no switch for another manifest, by design, and its TLS trust is macOS's
(`rustls-platform-verifier`). So the installed app ran unmodified, launched with the standard `HTTPS_PROXY` (proxy
support every shipped build has) pointing at a test proxy on the M1's loopback. The proxy answered only CONNECTs to
`api.getcmdr.com` and `getcmdr.com`, with leaf certs from a throwaway CA that David trusted in the M1's System keychain
(valid three days, name-constrained to `getcmdr.com` and its subdomains), and refused every other host. It mirrored
production: `/update-check/<v>` 302-redirects to `https://getcmdr.com/latest.json`, which served the test manifest
pointing at archives under `https://getcmdr.com/pipelinetest/`. So every line of the update path ran as shipped: the
real URL, real TLS verification, the redirect, the download, minisign, the code-signature check, the sync, and the
relaunch. Nothing reached production (the proxy saw one heartbeat, 10 checks, and 7 downloads, and no other host).
Rejected: a `/etc/hosts` entry (needs root, and still needs the CA), and a test-only manifest override (the installed
build would no longer be a release build).

**The "from" build**: this branch at `6b8039b55` with the version set to `0.52.1-pipelinetest.0`, built with
`pnpm tauri build --target aarch64-apple-darwin`, Developer ID signed on the laptop (not notarized), installed to
`~/Applications/Cmdr.app` (user-writable, so no admin prompt). Each launch first deleted `update-check.json` so the
startup check was due.

Archives served (each from the dry run's aarch64 archive; main-key signatures made on the laptop, deleted afterwards):

- **A, genuine**: the pipeline's archive with `release-finish`'s main-key signature. Check offered
  `0.52.1-pipelinetest.0 -> 0.52.1-pipelinetest.1`, "Signature verified", "Update installed successfully". After a quit
  and relaunch the process reported `current version: 0.52.1-pipelinetest.1`, the installed bundle passed
  `codesign --verify --deep --strict`, and `spctl` said "accepted, source=Notarized Developer ID" (the in-place sync
  reproduces the notarized bundle exactly). Still running 30 s later.
- **B, ad-hoc re-signed, main-key minisign** (what a leaked updater key alone can make): "Signature verified", then
  `ERROR updater::installer Refusing the update: its minisign signature verified, but the bundle isn't signed by Cmdr's Developer ID`,
  and the frontend's `update install refused, untrusted code signature`. App still `0.52.1-pipelinetest.0`, running,
  bundle intact.
- **C1, archive changed after signing** (one byte appended, genuine `.sig`): `signatureMismatch` at download, nothing
  staged. App unchanged and intact.
- **C2, one bundle byte changed, then main-key minisign**: "Signature verified", then refused with "the bundle's code
  signature doesn't hold (OSStatus -67030)" (`UntrustedCodeSignature`). App unchanged and intact.
- **D, signed by a throwaway minisign key**: `signatureMismatch` ("created with a different key") at download. App
  unchanged and intact.
- **After the refusals**, the same install (no reinstall, a refused attempt's staging dir still on disk) took A on its
  next launch and relaunched as `0.52.1-pipelinetest.1`.
- **The first hop real users take**: the published v0.52.0 aarch64 build (old updater, minisign only) installed into
  `~/Applications` took A, and relaunched as `0.52.1-pipelinetest.1`, accepted by Gatekeeper as notarized. From then on
  it runs the new updater, which is what A proved above.

The `cmdr-vet-update` gate also refused B and C2 on the laptop before any of this, so neither could have been signed by
`release-finish`.

## Step 4: the backup key

The real backup key (`B72B6B32B7D71B58`) is untested by design: its private half is only in David's Bitwarden. Coverage
with a throwaway key wired the same way: the app's `signature::tests::a_signature_from_any_trusted_key_verifies` puts
the signing key second in the list, where the backup sits, and the Go `TestVerifyAcceptsASignatureFromAnyTrustedKey`
does the same in both positions; `TestSigningEnvReadsTheBackupKeyOnlyFromTheEnvironment` covers `-backup-key` reading
only the environment. The code-signature check is independent of which key signed
(`crates/cmdr-update-trust/DETAILS.md`).

## Findings

- **`main`'s full CI run was red, which would have stopped the next release at the CI gate.** Two pre-existing breaks,
  both fixed on this branch: the API server's card-ratings tests (red since `369f699d2`: the fake GitHub matched
  `startsWith('query(')` while the real query is an indented `query (`), and three Linux E2E specs leaking the new
  "Opening <file>…" toast (since `da9546669`). The second full run went green.
- **`updater-backup.pub` was missing from the Rust lanes' embedded inputs** (since `00291264b`), so a lane could
  cache-skip an edit to it. Fixed in `scripts/check/checks/inputs.go`.
- **The gate's first CI run failed `cfg-gate`**: a crate-level `#![cfg(target_os = "macos")]` isn't something that check
  reads. Fixed by gating each module.
- **Follow-up idea, not done**: Tauri CLI 2.12's `signer sign` can bind a signature to a version (`--app-version`,
  checked by `requireSignedVersion`). Cmdr's own verifier doesn't read it today; the rollback defense is
  `refuse_unless_newer` on the bundle's `Info.plist`.

## Not tested

- **The `/Applications` install** (root-owned, so the sync escalates through an `osascript` admin prompt): needs a
  person to type a password. Every run here used `~/Applications`.
- **The restart toast's `relaunch()`**: each relaunch was a quit plus `open`, since the prod build has no MCP server to
  click it with. The installed bundle is what both launch.
- **The x64 and universal archives in an installed app**: only arm64 hardware was available. They passed the gate,
  `codesign`, `spctl`, and `stapler` statically.
- **The dispatched finishing run** (`publish`, `vet-update-archives` on a dispatch, `-verify-dir`): publishing a test
  tag would move `latest.json`, so it never ran. Its gate is the same binary the tag push ran.
- **A TLS negative** (an untrusted proxy CA): the CA was already trusted before the first launch.
- **The real backup key** (by design, above), and macOS versions other than 27.0 / 27.0.1.

## Cleanup

- GitHub: the draft release, the tags `v0.52.1-pipelinetest.1`, `pipelinetest-base-1`, and `pipelinetest-base-2`, and
  the run's four artifacts deleted. Left: the run's logs, and the `attest` job's provenance and SBOM attestations for
  the test assets (on the repo and in Sigstore's public log). Harmless: they describe assets that no longer exist
  anywhere.
- Laptop: the throwaway keys, doctored archives, and main-key signatures deleted; the two local test branches deleted.
- M1: `~/Applications/Cmdr.app`, the proxy, the CA's private key, and every test file removed; `/Applications/Cmdr.app`
  (0.52.0) untouched; no hosts entry was ever made. **Left for David**: the trusted test CA in the System keychain
  (inert without its deleted key, expires 2026-10-18): in a GUI session on the M1 (Remote Desktop terminal; sudo over
  ssh can't change trust settings),
  `sudo security delete-certificate -c "Cmdr update-path test CA 2026-10-15" /Library/Keychains/System.keychain`. The
  prod data dir there gained this test's log lines and an `update-check.json`.
