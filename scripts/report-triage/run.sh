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

gh_token="$(secret TRIAGE_GH_TOKEN)"
r2_key_id="$(secret TRIAGE_R2_ACCESS_KEY_ID)"
r2_secret="$(secret TRIAGE_R2_SECRET_ACCESS_KEY)"
prompt="$(cat "$here/prompt.md")"

cd "$work"
exec env -i \
  HOME="$HOME" \
  PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:$HOME/.local/bin" \
  GH_TOKEN="$gh_token" \
  GH_REPO="vdavid/cmdr-reports" \
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
