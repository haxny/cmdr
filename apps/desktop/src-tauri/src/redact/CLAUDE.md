# Redact

Path-shape-preserving redactor: one composed regex with named groups, one dispatch, ONE policy. Unsalted `redact_line`
(MCP resources, the crash hook) writes bare tokens (`<dir>`, `<host>`); `RedactionContext::redact_line` (reports) writes
tokens that correlate only within one report and process (key from an ephemeral process secret plus the report ID;
neither ships). Typed values use `RedactionContext::redact_path` / `redact_name` / …, which fail closed. Pattern table,
overlaps, and decisions: `DETAILS.md`.

## Must-knows

- **Path shape keeps only a fixed mount/home prefix, an allowlisted immediate parent, and a conservative extension.**
  Only a home-prefix path proves a home role (`HOME_ROLE_DIRS`: Downloads, Documents, Library, …), at any depth;
  extensionless leaves become `<dir>`.
- **A recognized remote reference is redacted as one unit**: scheme, hierarchy, address class, port, and extension
  stay; every identity gets its own token.
- **❌ Never branch the policy on `context.is_none()`.** The context only salts tokens. A second, byte-compatible MCP
  policy once let a space-containing URL swallow a following path (cmdr-reports#30);
  `unsalted_api_runs_the_report_policy_with_bare_tokens` pins the two together.
- **Only exact derived-ID shapes are recognized** (known scheme + 16-hex digest, legacy `manual-…-<port>`). Never guess
  from arbitrary hyphens.
- **Log a relative path or name as `key={:?}` with a key from `path_field`**, and an identity as `host={host:?}` (or
  `server`, `share`, `user`, the ID keys). Bare prose is invisible to the redactor; literal wrapper quotes let a value
  escape its field.
- **External OS/server/CLI text goes in `detail={:?}` / `stderr={:?}` / `stdout={:?}` via `cmdr_fs::log_detail::LogDetail`**,
  next to its machine code (`code=`, `nt_status=`, `sftp_status=`). Local logs keep it whole; a report redacts it, scrubs
  the line's own keyed identities from it, and caps it at 200 chars. ❌ Never redact at the log site, never drop the
  text. `DETAILS.md` § "External-text fields".
- **A prose path runs to the next hard delimiter** (`split_trailing_noise`): the `: ` seam, a new reference, ` -> `,
  and `, ` / `; ` / ` key=` after the last separator. Prose after it goes with it. ❌ Never end a path on a word's shape
  (a `)`, a dot, a lowercase run): each such rule shipped part of a name (`Summer (Lake House) 2026` lost ` 2026`).
  `DETAILS.md` § "Finding the end of a path".
- **Tokens key on a domain plus the normalized name** (escapes undone, NFC, Cmdr temp suffix split off). Typed inputs
  never trust token-looking syntax.
- **`redact_with` resumes at `match.start() + consumed`, ❌ never `replace_all`.** A handed-back tail must face the
  scanner again: one match once ate `smb:` and shipped the share and filename. A branch that hands text back owes
  `dispatch` a consumed length.

## Gotchas

- **A bare name is redacted only when its line also names it in a path** (the leaf echo scrub): `the Trash refused it:
  “Screenshot ….jpeg”` goes because the same line logs the file's path. A name with no path or key on its line still
  ships, so key it (`file={name:?}`). Pinned by `a_path_leaf_repeated_bare_on_its_line_is_scrubbed`.
- **Any absolute path is a path** (`abs_path`, `\B/` plus two segments), whatever its prefix; a lone `/foo` stays prose.
- **OS junk names ship verbatim** (`.DS_Store`, `Thumbs.db`, `.Trashes`, …; `paths.rs::OS_JUNK_NAMES`), exact segment
  match only.
- **Dispatch order mirrors regex alternation order.** Complete URLs before generic userinfo, derived IDs before IPs.

## Files

`mod.rs` (API, regex, dispatch), `context.rs` (report key, token domains), `paths.rs`, `references.rs` (remote
references, derived IDs), `fields.rs` (keyed fields), `detail.rs` (external-text fields), `path_end.rs` (where a
prose path ends), `names.rs` (printing normalization); tests in `tests.rs`, `reference_tests.rs`, `detail_tests.rs`, `prose_tests.rs`,
`consistency_tests.rs`, `bundle_leak_tests.rs` (shapes that shipped in real bundles), golden corpus in `fixtures/`.
