#!/usr/bin/env bash
# Disables every workflow the fork inherited from vdavid/cmdr (CI, release, CodeQL, deploys, ...)
# so only `haxny-fork-build.yml` runs in the fork. Upstream's need secrets, self-hosted runners,
# or deploy keys the fork doesn't have, and would fail (or burn minutes) on every push.
#
# Usage: .github/haxny/disable-inherited-workflows.sh [owner/repo]   (default haxny/cmdr)
#        DRY_RUN=1 .github/haxny/disable-inherited-workflows.sh       (print, change nothing)
# Needs `gh` logged in with admin rights on the fork. Re-run after a merge brings a new workflow.
set -euo pipefail

repo=${1:-haxny/cmdr}
keep=haxny-fork-build.yml

gh workflow list --repo "$repo" --all --json name,path,state --jq '.[] | [.path, .state, .name] | @tsv' |
    while IFS=$'\t' read -r path state name; do
        file=${path##*/}
        if [ "$file" = "$keep" ]; then
            echo "keep     $file ($state)"
        elif [ "$state" != active ]; then
            echo "skip     $file ($state)"
        elif [ "${DRY_RUN:-}" = 1 ]; then
            echo "would disable $file ($name)"
        else
            if gh workflow disable "$file" --repo "$repo"; then
                echo "disabled $file ($name)"
            else
                echo "COULD NOT disable $file ($name)" >&2
            fi
        fi
    done
