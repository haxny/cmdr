#!/usr/bin/env bash
# Download one error-report bundle from R2 and unzip it, for the triage agent.
#
# Usage: ./fetch-bundle.sh <r2-key> <dest-dir>
#   <r2-key> is the card's "R2 key", like error-reports/prod/2026-10-14/ERR-AB7PV-<uuid>.zip
#
# Reads TRIAGE_R2_ACCOUNT_ID, TRIAGE_R2_ACCESS_KEY_ID, and TRIAGE_R2_SECRET_ACCESS_KEY from the environment: an R2 API
# token with Object Read on the cmdr-error-reports bucket ONLY, so a prompt-injected agent can read bundles and
# nothing else. Signs with curl's built-in SigV4, so there's no SDK to install.
set -euo pipefail

key="${1:?usage: fetch-bundle.sh <r2-key> <dest-dir>}"
dest="${2:?usage: fetch-bundle.sh <r2-key> <dest-dir>}"

case "$key" in
  *..* | *//*)
    echo "Refusing: the key must not contain '..' or '//'" >&2
    exit 2
    ;;
  error-reports/prod/*.zip | error-reports/prod/*.amend.json) ;;
  *)
    echo "Refusing: only error-reports/prod/ bundles and sidecars can be fetched" >&2
    exit 2
    ;;
esac

mkdir -p "$dest"
out="$dest/$(basename "$key")"
curl --fail --silent --show-error --max-time 60 \
  --aws-sigv4 "aws:amz:auto:s3" \
  --user "${TRIAGE_R2_ACCESS_KEY_ID}:${TRIAGE_R2_SECRET_ACCESS_KEY}" \
  --output "$out" \
  "https://${TRIAGE_R2_ACCOUNT_ID}.r2.cloudflarestorage.com/cmdr-error-reports/${key}"

if [[ "$out" == *.zip ]]; then
  unzip -q -o "$out" -d "$dest/$(basename "$key" .zip)"
  echo "$dest/$(basename "$key" .zip)"
else
  echo "$out"
fi
