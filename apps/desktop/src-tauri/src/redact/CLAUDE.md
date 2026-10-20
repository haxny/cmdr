# Redact

Path-shape-preserving redactor: a span pass for values producers marked, then one composed regex, one dispatch, ONE
policy. Unsalted `redact_line` (MCP resources, the crash hook) writes bare tokens (`<dir>`);
`RedactionContext::redact_line` (reports) writes tokens that correlate only within one report and process. Typed
values use `RedactionContext::redact_path` / `redact_name` / …, which fail closed. Pattern table and decisions:
`DETAILS.md`.

## Must-knows

- **Marked spans go first** (`markers.rs`): a value between a `cmdr_fs::log_private` opener and closer is hashed by its
  producer's kind, at any `{:?}` depth, before the regex sees the line. Depth is `1 + trailing_zeros` of the backslash
  run, and a span closes only at its own depth, so a hostile name can't close it early. A broken span fails closed (to
  the line's or field's end; a stray closer back to the last boundary). ❗ The regex reads a placeholder per span, ❌
  never the token, or it re-walks the span's output. `DETAILS.md` § "Marked spans".
- **Path shape keeps only a fixed mount/home prefix, an allowlisted immediate parent, and a conservative extension.**
  Only a home-prefix path proves a home role (`HOME_ROLE_DIRS`); extensionless leaves become `<dir>` unless a span said
  file.
- **A recognized remote reference is redacted as one unit**: scheme, hierarchy, address class, port, and extension
  stay; every identity gets its own token.
- **❌ Never branch the policy on `context.is_none()`.** The context only salts tokens; a second MCP policy once let a
  URL swallow a following path (cmdr-reports#30). `unsalted_api_runs_the_report_policy_with_bare_tokens` pins it.
- **Only exact derived-ID shapes are recognized** (known scheme + 16-hex digest, legacy `manual-…-<port>`).
- **Mark a value at its log site with `LogPrivate`** (a path-carrying error with `LogError`), keeping `key=` and spec.
  A crate that can't depend on `cmdr-fs` logs a relative path or name as `key={:?}` with a key from `path_field`, an
  identity as `host={host:?}` (or `server`, `share`, `user`, the ID keys). Bare prose is invisible to the regex.
- **External OS/server/CLI text goes in `detail={:?}` / `stderr={:?}` / `stdout={:?}` via
  `cmdr_fs::log_detail::LogDetail`**, next to its machine code; `.marking(&[…])` marks the values the caller handed
  over. A report redacts, echo-scrubs, and caps it after span hashing. ❌ Never redact at the log site. `DETAILS.md` §
  "External-text fields".
- **A prose path runs to the next hard delimiter** (`split_trailing_noise`): the `: ` seam, a new reference, ` -> `,
  and `, ` / `; ` / ` key=` after the last separator. ❌ Never end a path on a word's shape (a `)`, a dot, a lowercase
  run): each such rule shipped part of a name. `DETAILS.md` § "Finding the end of a path".
- **Tokens key on a domain plus the normalized name** (escapes undone, NFC, Cmdr temp suffix split off). Typed inputs
  never trust token-looking syntax.
- **`redact_with` resumes at `match.start() + consumed`, ❌ never `replace_all`.** A handed-back tail must face the
  scanner again. A branch that hands text back owes `dispatch` a consumed length.

## Gotchas

- **A bare name goes only when its line also names it** in a path, a span, or a key (the leaf echo scrub, never the
  log header). Pinned by `a_path_leaf_repeated_bare_on_its_line_is_scrubbed`.
- **Any absolute path is a path** (`abs_path`, `\B/` plus two segments); OS junk names (`.DS_Store`, …) ship verbatim.
- **Dispatch order mirrors regex alternation order.** Complete URLs before generic userinfo, derived IDs before IPs.

## Files

`mod.rs` (API, regex, dispatch), `markers.rs` (the span pass, and the sinks' `display_line`),
`context.rs` (report key, token domains), `paths.rs`, `references.rs`, `fields.rs` (keyed fields), `detail.rs`
(external text, echo scrub), `path_end.rs`, `names.rs`, leaves `tokens.rs`, `placeholders.rs`, `echoes.rs`; tests in `*tests.rs` (`bundle_leak_tests.rs`: shapes that
shipped in real bundles), golden corpus in `fixtures/`.
