#!/usr/bin/env bash
# The morning report triager, run headless on the M1 box at 05:30 Stockholm time by launchd, an hour before the API
# server's morning brief. See CLAUDE.md here for what it does and how it's locked down.
#
# Secrets come from the M1 box's own `secret` store, resolved BEFORE `env -i`, so the agent's process holds exactly
# two scoped credentials and nothing else from the box's environment:
#   TRIAGE_GH_TOKEN           fine-grained PAT: vdavid/cmdr-reports ONLY, Issues read+write, Metadata read
#   TRIAGE_R2_ACCESS_KEY_ID   R2 API token: Object Read on the cmdr-error-reports bucket ONLY
#   TRIAGE_R2_SECRET_ACCESS_KEY
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work="$HOME/report-triage"
mkdir -p "$work"

# A fresh, shallow, public checkout of Cmdr for code research, kept apart from any clone David works in.
if [[ -d "$work/cmdr/.git" ]]; then
  git -C "$work/cmdr" fetch --quiet --depth 1 origin main
  git -C "$work/cmdr" reset --quiet --hard origin/main
else
  git clone --quiet --depth 1 https://github.com/vdavid/cmdr "$work/cmdr"
fi
cp "$here/fetch-bundle.sh" "$work/fetch-bundle.sh"
chmod +x "$work/fetch-bundle.sh"
# An empty gh config dir, fresh every run: gh refuses to start without a readable one, the sandbox denies the real
# `~/.config/gh`, and GH_TOKEN is all it needs. Recreated so nothing (an alias, say) carries over between runs.
rm -rf "$work/.gh-config"
mkdir -p "$work/.gh-config"

gh_token="$(secret TRIAGE_GH_TOKEN)"
r2_key_id="$(secret TRIAGE_R2_ACCESS_KEY_ID)"
r2_secret="$(secret TRIAGE_R2_SECRET_ACCESS_KEY)"
# Claude Code's own login. Over SSH and under launchd the login keychain isn't reliably unlocked, so the headless
# run authenticates with the box's OAuth token. settings.json strips it from every sandboxed command's environment
# (`sandbox.credentials.envVars`), so only the claude process itself holds it.
claude_token="$(secret CLAUDE_CODE_OAUTH_TOKEN)"
prompt="$(cat "$here/prompt.md")"

# The real `gh` binary, not a mise shim: a shim runs mise, which reads `~/.config/mise`, and the sandbox denies the
# whole home directory except the working dir and gh's install dir (settings.json).
gh_bin="$(mise which gh 2>/dev/null || command -v gh)"
gh_dir="$(dirname "$gh_bin")"

{
  echo "=== report-triage run $(date '+%Y-%m-%d %H:%M:%S %Z') ==="
  echo "claude $(claude --version 2>/dev/null), gh at $gh_bin"
} >&2

cd "$work"
# SHELL + ZDOTDIR: Claude Code snapshots the user's shell setup (outside the sandbox) and replays it in every Bash
# command, so without these the box's `~/.zshenv` re-exports its broad GH_TOKEN (over ours), CLOUDFLARE_API_TOKEN, and
# the rest straight back into the agent's shell, undoing `env -i`. `/var/empty` is a root-owned empty dir, so zsh
# finds no startup files to read (verified on the M1, Claude Code 2.1.299, 2026-10-14).
exec env -i \
  HOME="$HOME" \
  SHELL=/bin/zsh \
  ZDOTDIR=/var/empty \
  PATH="$gh_dir:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:$HOME/.local/bin" \
  CLAUDE_CODE_OAUTH_TOKEN="$claude_token" \
  GH_TOKEN="$gh_token" \
  GH_REPO="vdavid/cmdr-reports" \
  GH_CONFIG_DIR="$work/.gh-config" \
  GH_NO_UPDATE_NOTIFIER=1 \
  TRIAGE_R2_ACCOUNT_ID="6a4433bf11c3cf86feda057f76f47991" \
  TRIAGE_R2_ACCESS_KEY_ID="$r2_key_id" \
  TRIAGE_R2_SECRET_ACCESS_KEY="$r2_secret" \
  claude -p "$prompt" \
  --model claude-opus-5-5 \
  --settings "$here/settings.json" \
  --setting-sources project,local \
  --strict-mcp-config --mcp-config '{"mcpServers":{}}' \
  --tools Bash,Read,Grep,Glob \
  --permission-mode dontAsk \
  --permission-prompts none
