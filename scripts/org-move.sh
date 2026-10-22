#!/bin/bash
# Rewrites the repo's identity from `vdavid/cmdr` to `getcmdr/cmdr` for the move to the `getcmdr`
# GitHub org (#476). Idempotent; prints what it changed and what it kept. What it does and why:
# `scripts/org-move/main.go`; when to run it: `docs/notes/github-org-move.md`.
#
# Usage: ./scripts/org-move.sh -cutoff vX.Y.Z [-dry-run] [-v]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}/scripts/org-move"
exec go run . -root "${ROOT}" "$@"
