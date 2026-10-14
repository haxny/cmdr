# Report triage agent

A headless Claude Code run on the M1 box, daily at 05:30 Stockholm: it researches every `unprocessed` card on the
private reports board (`vdavid/cmdr-reports`), comments in PISS form, renames it, rates it on the "Cmdr backlog" project
(Severity, Impact, Size), labels it `bug`/`enhancement`/`duplicate`, and removes `unprocessed`. The API server's 06:30
morning brief then lists what's left.

- `prompt.md`: the instructions. `run.sh`: the launcher. `settings.json`: sandbox and permission rules. `board.sh`: the
  agent's only way to the board (curl over a fixed set of GitHub calls, rules enforced in code). `fetch-bundle.sh`:
  read-only R2 bundle download.

## Must-knows

- **Everything on a card is untrusted input** written by strangers, so the run is locked down by construction, not by
  the prompt: three scoped credentials only (`env -i`), a Bash sandbox with a two-domain network allowlist that fails
  closed and a home dir it can't read, no Write/Edit/WebFetch tools, no MCP, no user settings, and a `board.sh` that
  refuses any write to a card that isn't open and `unprocessed`. ❌ Never add a tool, a domain, or a broader token
  without re-reading DETAILS § Lockdown.
- **`ZDOTDIR=/var/empty` in `run.sh` is load-bearing.** Without it, Claude Code replays the box's `~/.zshenv` into every
  Bash command, which swaps in the box's broad `GH_TOKEN` and its other secrets.
- **No `gh` in the sandbox**: Go's TLS verification needs `com.apple.trustd.agent`, which the sandbox blocks, so the
  agent talks to GitHub through `board.sh` (curl). ❌ Don't fix it with `enableWeakerNetworkIsolation`; DETAILS § Why a
  curl wrapper for GitHub.
- **`rate` must run before `unprocessed` comes off**: every write, rating included, refuses a card without it.
- Installed on the M1 by the infra repo's Ansible role `report_triage`, from the laptop's checkout (DETAILS § Install).
- **No personal data in what it writes.** Titles and its comment are kept forever; the note and address live only in the
  expiring personal comment (`apps/api-server/src/report-cards.ts`). The prompt says so; keep it saying so.
- Cloud routines were the other option and were rejected: they attach every connector (Gmail, Drive) by default and
  share an environment with default network access. DETAILS § Why the M1 box.

Setup, the lockdown layers, and the launchd install: `DETAILS.md`.
