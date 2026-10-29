#!/usr/bin/env bash
# Merges an upstream tag into the checked-out branch with --no-ff. A conflict confined to files the
# fork build regenerates right after the merge (IPC bindings, message-key types) is resolved by
# taking upstream's side: the regeneration step then rewrites them from the merged sources. Any
# other conflict aborts the merge, writes the conflicted paths to `<conflicts-file>`, and exits 1.
#
# Usage: .github/haxny/merge-upstream.sh <tag> <conflicts-file> [--trial]
#   --trial  stop before committing and undo the merge (the workflow's cheap pre-check)
set -euo pipefail

if [ $# -lt 2 ]; then
    echo "usage: $0 <tag> <conflicts-file> [--trial]" >&2
    exit 2
fi
tag=$1
conflicts_file=$2
trial=${3:-}

# Rewritten by `pnpm bindings:regen` and `pnpm intl:keys` in the workflow, so either side will do.
regenerated=(
    apps/desktop/src/lib/ipc/bindings.ts
    apps/desktop/src/lib/intl/keys.gen.ts
)

: > "$conflicts_file"
branch=$(git rev-parse --abbrev-ref HEAD)
if git merge --no-ff --no-commit "refs/tags/$tag" > /dev/null 2>&1; then
    clean=true
else
    clean=false
    git diff --name-only --diff-filter=U | while read -r path; do
        keep=false
        for gen in "${regenerated[@]}"; do
            [ "$path" = "$gen" ] && keep=true
        done
        if $keep; then
            git checkout --theirs -- "$path"
            git add -- "$path"
            echo "Took upstream's $path (regenerated after the merge)."
        else
            echo "$path" >> "$conflicts_file"
        fi
    done
fi

if [ -s "$conflicts_file" ]; then
    git merge --abort
    echo "Merging $tag conflicts in:" >&2
    sed 's/^/  /' "$conflicts_file" >&2
    exit 1
fi

if [ "$trial" = --trial ]; then
    git merge --abort 2>/dev/null || git reset --hard -q HEAD
    echo "$tag merges $($clean && echo cleanly || echo 'after taking upstream regenerated files')."
    exit 0
fi

git commit -q --no-edit -m "merge: upstream $tag into $branch"
echo "Merged $tag into $branch."
