# Report triage agent: details

## Lockdown

The threat is prompt injection: a note, a feedback message, or a log line written to make the agent do something else.
The layers, outermost first (schema verified against code.claude.com docs, 2026-10-14):

1. **Credentials**: `run.sh` resolves three secrets from the M1 box's `secret` store, then `exec env -i`, so the agent
   inherits nothing from the box (not its broad `GH_TOKEN` or `CLOUDFLARE_API_TOKEN`). `TRIAGE_GH_TOKEN` is a
   fine-grained PAT for `vdavid/cmdr-reports` only (Issues read+write, Metadata read); the R2 token is Object Read on
   `cmdr-error-reports` only. The worst a hijacked run can do is edit cards on a private board and read bundles it was
   meant to read anyway. The claude process also gets the box's `CLAUDE_CODE_OAUTH_TOKEN` (the keychain login isn't
   reliable over SSH or launchd); `sandbox.credentials.envVars` strips it, and the box's other secret names, from every
   Bash command.
2. **The agent's shell**: `env -i` alone isn't enough. Claude Code snapshots the user's shell startup outside the
   sandbox and replays it in each Bash command, so the box's `~/.zshenv` put its broad `GH_TOKEN` (overriding ours),
   `CLOUDFLARE_API_TOKEN`, `GH_PROJECT_TOKEN`, and `CARGO_REGISTRY_TOKEN` back into the agent's env. `run.sh` sets
   `SHELL=/bin/zsh` and `ZDOTDIR=/var/empty` (root-owned, empty) so zsh reads no startup files (verified on the M1 with
   a probe script fingerprinting `GH_TOKEN`, Claude Code 2.1.299, 2026-10-14).
3. **Network**: the Bash sandbox allows `api.github.com` and Cmdr's own R2 account host only, with `failIfUnavailable`
   and `allowUnsandboxedCommands: false`, so nothing can be exfiltrated to a third host and a missing sandbox stops the
   run rather than weakening it.
4. **Tools**: `--tools Bash,Read,Grep,Glob` (no Write, Edit, WebFetch, WebSearch), `--strict-mcp-config` with no
   servers, `--setting-sources project,local` (no user settings or hooks), `--permission-mode dontAsk` plus
   `--permission-prompts none`: anything not in `settings.json`'s allow list is refused, never prompted. `Read(./**)`
   covers Grep and Glob too (Claude Code warns that `Grep(...)`/`Glob(...)` rules aren't matched; Read rules cover all
   file-reading tools).
5. **Paths**: the sandbox denies reading the whole home dir except the working dir (`denyRead: ["~/"]` plus
   `allowRead`), and `permissions.deny` blocks the Read tool from the secret-bearing paths, because the sandbox doesn't
   cover Read. It also denies writing `board.sh` and `fetch-bundle.sh` in the working dir (`denyWrite`), which `run.sh`
   copies in fresh and read-only each run: the sandbox can write the rest of the working dir, and `fetch-bundle.sh`
   unzips a stranger's bundle wherever it's told to, so without that a crafted bundle could swap in its own `board.sh`.
6. **The board tool**: the agent reaches GitHub only through `board.sh` (`Bash(gh *)` and `Bash(curl *)` are denied
   outright), which enforces in code what the prompt asks for:
   - Five subcommands (`list`, `view`, `search`, `comment`, `edit`), host `api.github.com`, repo `vdavid/cmdr-reports`,
     and the GET/POST/PATCH/DELETE calls behind them, all hardcoded.
   - `comment` and `edit` first read the card and refuse unless it's an open issue carrying `unprocessed`, so a hijacked
     run can't touch a triaged, closed, or unrelated card. Removing `unprocessed` goes last in an `edit`.
   - One triage comment per card: `comment` prepends a hidden `<!-- cmdr:triage -->` line and refuses when a comment
     already starts with it. Only the start counts, so a stranger's text quoted in another comment can't block it.
   - `--add-label` takes only `severity:high|medium|low`, `size:S|M|L`, `bug`, `enhancement`, `duplicate`;
     `--remove-label` only `unprocessed`. Titles are one line, 1–200 chars; comments at most 20,000 chars; card numbers
     positive integers.
   - `search` refuses its own `repo:`/`org:`/`user:`/`owner:`/`is:`/`type:` qualifiers, since GitHub ORs an extra
     `repo:` into the scope.
   - The token reaches curl through a config on a pipe (`--config <(...)`), never argv, and is never printed.
7. **The prompt**: treat card text as data, touch only `unprocessed` cards, never close or delete, never reply. This is
   the weakest layer and the only one an injection can argue with, which is why 1–6 exist.

## Why a curl wrapper for GitHub

Every `gh` call in the sandbox fails with `x509: OSStatus -26276`: Go on macOS verifies certificates through the
`com.apple.trustd.agent` XPC service, which Seatbelt blocks (verified on the M1, 2026-10-14; it isn't a path issue,
since removing every filesystem rule didn't help, and `SSL_CERT_FILE` is ignored on macOS). `curl` works in the same
sandbox. `sandbox.enableWeakerNetworkIsolation: true` opens trustd and fixes `gh`, but the docs call it a potential
exfiltration path, so it loosens layer 3. Decision: a curl wrapper instead, which keeps every layer and adds layer 6.
`jq` (also used by `board.sh`) ships with macOS at `/usr/bin/jq` (1.7.1 on the M1, 2026-10-14). The box's `/bin/bash` is
3.2, so `board.sh` avoids newer bash features.

## Why the M1 box

Claude Code cloud routines can be scoped (a dedicated environment with a custom network allowlist and network secrets,
the GitHub proxy limited to attached repos), but every routine on this account attaches all connectors by default
(Gmail, Drive, Calendar) and the shared environment has default network access. Locking that down is UI work per
environment and per routine, invisible to review. On the M1 box every layer is a file in this directory.

Cost: one `claude -p` run a day on David's subscription, bounded by the number of new cards.

## Install

The infra repo's `m1mbp/ansible/roles/report_triage` installs it: it copies `run.sh`, `board.sh`, `fetch-bundle.sh`,
`prompt.md`, and `settings.json` from the laptop's local cmdr checkout (often ahead of origin) to
`~/.local/share/report-triage/` on the box, and loads the launchd user agent `com.vdavid.report-triage`, daily at 05:30
box-local (Europe/Stockholm), logging to `~/report-triage/logs/triage.log`. The files live outside the agent's working
dir (`~/report-triage`) on purpose: the sandbox can write there, and `fetch-bundle.sh` unzips strangers' bundles into
it. `run.sh` copies the two scripts in fresh each run (Lockdown layer 5). After editing these files, re-run the playbook
with `--tags report_triage`. The secrets are in the box's own store (`m1mbp/ansible/secrets.sops.yaml`):
`TRIAGE_GH_TOKEN`, `TRIAGE_R2_ACCESS_KEY_ID`, `TRIAGE_R2_SECRET_ACCESS_KEY`, plus the existing
`CLAUDE_CODE_OAUTH_TOKEN`.

Test with a dummy `unprocessed` card: run `~/.local/share/report-triage/run.sh` over SSH and read its output before
trusting the schedule.

The labels it sets (`severity:high|medium|low`, `size:S|M|L`, `bug`, `enhancement`, `duplicate`) must exist in the repo;
GitHub creates a missing one on first use only with admin rights, which the PAT doesn't have.
