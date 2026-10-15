# cmdr-update-trust

What makes a macOS update archive a Cmdr release: it unpacks, with the installer's own extraction, to a `Cmdr.app`
code-signed by Cmdr's Developer ID (team `83H6YAQMNP`). The app's updater runs it before installing; the release gate
(`cmdr-vet-update`) runs it before an archive is signed or published. One home, so the two can't disagree.

## Module map

- `code_signature.rs`: `verify_release_bundle`, Security.framework against `RELEASE_REQUIREMENT`, typed
  `CodeSignatureRefusal`.
- `archive.rs`: `extract_tarball` (the installer's extraction), `bundle_version`, and `vet_update_archive`, the gate.
- `main.rs`: the `cmdr-vet-update` binary. `fixtures.rs`: `codesign`-built test bundles.

## Must-knows

- ❌ **Nothing may skip the check**: no env var, manifest field, setting, or flag. Tests inject it (`vet_with`, and the
  app's `vet_staged_bundle`). `DETAILS.md` § Why there's no switch.
- ❗ **The gate must extract with `extract_tarball`**, never the system `tar`: the `tar` crate drops xattrs and sets its
  own modes, so a bundle can verify after one and not the other.
- ❗ **The requirement is byte for byte what every release carries** (`codesign -d -r-`). Changing the bundle ID or the
  team stops every update and every FDA grant.
- ❗ **macOS only**: the lib is empty elsewhere and the binary exits 2.
- **Run the opt-in real-release test** before changing the requirement, the flags, or the extraction (`DETAILS.md` §
  Verified against real releases).

Rationale, the requirement's clauses, the gate's two callers, and the verification record: `DETAILS.md`.
