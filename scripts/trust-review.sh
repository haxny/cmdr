#!/bin/bash
# Lists the getcmdr.com/trust sections whose source changed since the page was last verified, or
# validates the source map behind that (`docs/guides/trust-page-review.md`). What each subcommand
# does: `scripts/trust-review/main.go`.
#
# Usage: ./scripts/trust-review.sh [changes] [-since <ref>] [-to <ref>]
#        ./scripts/trust-review.sh validate
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}/scripts/trust-review"
exec go run . -root "${ROOT}" "$@"
