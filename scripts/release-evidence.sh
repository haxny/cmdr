#!/bin/bash
# Checks a published release's public surface and collects its CRA evidence (steps 12 and 13 of
# `docs/guides/releasing.md` § The release procedure). What each subcommand does:
# `scripts/release-evidence/main.go`.
#
# Usage: ./scripts/release-evidence.sh verify <version>
#        ./scripts/release-evidence.sh evidence [-archive //host/share/path] [-out DIR] <version>
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}/scripts/release-evidence"
exec go run . -root "${ROOT}" "$@"
