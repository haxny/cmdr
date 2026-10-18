#!/bin/bash
set -euo pipefail

VERSION="${1:-}"

if [[ -z "$VERSION" ]]; then
  echo "Usage: ./scripts/release.sh <version>"
  echo "Example: ./scripts/release.sh 0.2.1"
  exit 1
fi

# Validate version format
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Error: Version must be in format X.Y.Z (e.g., 0.2.1)"
  exit 1
fi

# `main` only takes commits CI passed, so the release commit goes through `pnpm land` like any other
# change: it's made on this branch, landed by fast-forward (same SHA), and only then tagged.
RELEASE_BRANCH="release/v$VERSION"
RELEASE_SUBJECT="chore(release): v$VERSION"

# Everything up to the release commit, on `main` in the main clone. Ends on $RELEASE_BRANCH with the
# commit made.
prepare_release_commit() {
  if [[ "$(git branch --show-current)" != main ]]; then
    echo "Error: run this on main (in the main clone), which it branches $RELEASE_BRANCH from."
    exit 1
  fi

  # Check for uncommitted changes. The release-prep edits are allowed through and get included in
  # the release commit: the changelog, the roadmap (content in roadmap.ts, layout in roadmap.astro),
  # the feature-status source of truth, and the security advisory list (its new entry names this
  # version, which the `desktop-security-advisories` check only accepts once the bump below lands).
  # See docs/guides/releasing.md § The release procedure for the steps that produce them.
  local exclude=(
    ':!CHANGELOG.md'
    ':!feature-status.json'
    ':!apps/website/src/lib/roadmap.ts'
    ':!apps/website/src/pages/roadmap.astro'
    ':!apps/desktop/src-tauri/resources/security-advisories.json'
  )
  if ! git diff --quiet -- "${exclude[@]}" || ! git diff --staged --quiet -- "${exclude[@]}"; then
    echo "Error: Working tree has uncommitted changes (other than the changelog, roadmap, feature status, and security advisory list). Commit or stash them first."
    exit 1
  fi

  # Detach stale Cmdr* DMG mounts. The self-hosted runner builds on this same Mac;
  # a leftover /Volumes/Cmdr (typically from inspecting an old DMG in Finder) makes
  # `bundle_dmg.sh` fail fast on the runner because the volume name is already
  # taken. The release workflow has the same guard before its bundle step,
  # but failing here means we don't tag a release that can't ship.
  while IFS= read -r vol; do
    if [[ -n "$vol" ]]; then
      echo "Detaching stale mount: $vol"
      hdiutil detach "$vol" -force >/dev/null 2>&1 || true
    fi
  done < <(mount | awk -F' on ' '/\/Volumes\/Cmdr/ { sub(/ \(.*$/, "", $2); print $2 }')

  # Stage allowed files so the pull below carries them along
  git add CHANGELOG.md feature-status.json apps/website/src/lib/roadmap.ts apps/website/src/pages/roadmap.astro apps/desktop/src-tauri/resources/security-advisories.json 2>/dev/null || true

  # Local `main` mirrors `origin/main`, so this only ever fast-forwards. A local commit on `main`
  # makes the CI gate below refuse: it belongs on a branch, landed through `pnpm land`.
  # --autostash: temporarily stashes the staged release-prep changes so the pull can proceed
  git pull --ff-only --autostash origin main

  # Check CHANGELOG.md has an [Unreleased] section with content
  if ! grep -q '## \[Unreleased\]' CHANGELOG.md; then
    echo "Error: CHANGELOG.md has no [Unreleased] section."
    echo "Add a '## [Unreleased]' heading with release notes before the first versioned section."
    exit 1
  fi
  local unreleased
  unreleased=$(sed -n '/## \[Unreleased\]/,/## \[/p' CHANGELOG.md | sed '1d;$d' | grep -v '^$' || true)
  if [[ -z "$unreleased" ]]; then
    echo "Error: The [Unreleased] section in CHANGELOG.md is empty."
    echo "Add release notes under it before releasing!"
    exit 1
  fi

  # Every changelog ref must be a commit on `main` (reachable from HEAD), or the website links it
  # to nothing. `--fresh`: its cache keys on CHANGELOG.md alone, and reachability is about history.
  pnpm check changelog-links -m --fresh

  # Release gate: only a commit with a fully green CI run ships (a `run_all` run, since push runs
  # skip unchanged lanes). It also refuses a HEAD that isn't origin/main, which CI never saw. Starts
  # the run if the release procedure didn't already, and waits for it. See docs/guides/releasing.md.
  ./scripts/release-ci-gate.sh

  echo "Releasing version $VERSION..."

  # Update version in package.json
  (cd apps/desktop && npm pkg set version="$VERSION")

  # Record the settings defaults THIS release ships with, so the analytics dashboard can tell
  # "the user is on the default" from "the setting didn't exist yet" for installs on it. The
  # manifest only gains an entry when a default actually moved, but every run stamps
  # `promotedThrough`, which is how the check tells a release that changed nothing from one nobody
  # recorded. Skipping it fails `analytics-settings-defaults` on the next check run.
  (cd apps/desktop && node scripts/gen-analytics-defaults.ts --promote "$VERSION")

  # Update version in tauri.conf.json
  (cd apps/desktop/src-tauri && jq ".version = \"$VERSION\"" tauri.conf.json > tauri.conf.json.tmp && mv tauri.conf.json.tmp tauri.conf.json)

  # Update version in Cargo.toml and sync Cargo.lock
  sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" apps/desktop/src-tauri/Cargo.toml
  (cd apps/desktop/src-tauri && cargo update --workspace --quiet)

  # Update CHANGELOG.md: replace [Unreleased] with the versioned heading
  local today
  today=$(date +%Y-%m-%d)
  sed -i '' "s/## \[Unreleased\]/## [$VERSION] - $today/" CHANGELOG.md

  # Roll the BSL Change Date forward so THIS release converts to AGPL three years after it
  # ships. The Change Date is a static field, not a rolling window: left alone, every version
  # ever shipped converts on one shared date, and the protection window shrinks with every
  # release (a build shipped in December 2028 against a 2029-01-10 date would go AGPL a month
  # later). BSL takes whichever of the Change Date and the version's fourth anniversary comes
  # FIRST, so the anniversary can only ever pull conversion earlier; three years leaves a year
  # of headroom under that four-year cap.
  local change_date
  change_date=$(date -v+3y +%Y-%m-%d)
  sed -i '' "s/^Change Date:.*/Change Date:          $change_date/" LICENSE

  # Refresh the website's visual baselines against the finalized release copy, so a release can't
  # ship a stale one and turn CI red on the release commit. The baselines are region-scoped and no
  # longer cover /features, so release-prep copy shouldn't move any of them; this stays as a cheap
  # guard, and the `git add -u` below stages whatever actually moved. Requires Docker (the Linux
  # baselines can't be rendered on macOS); a missing/stopped Docker aborts here, before committing.
  apps/website/scripts/update-visual-baselines.sh

  # Run oxfmt across the repo so CHANGELOG / package.json / tauri.conf.json drift from
  # manual edits + the sed/`npm pkg set` mutations above doesn't fail CI on the release commit.
  # This also reformats any unrelated files that drifted (for example, a `.claude/commands/*.md`
  # touched in the same uncommitted batch the user is releasing).
  # `-m`: the release runs in the main clone by design, so opt past the worktree-only
  # guard (the `--ci` gate below opts past it via --ci instead).
  pnpm check oxfmt -m

  # Stage the files the script itself just bumped.
  git add \
    CHANGELOG.md \
    LICENSE \
    feature-status.json \
    apps/website/src/lib/roadmap.ts \
    apps/website/src/pages/roadmap.astro \
    apps/desktop/src-tauri/resources/security-advisories.json \
    apps/desktop/package.json \
    apps/desktop/src-tauri/tauri.conf.json \
    apps/desktop/src-tauri/Cargo.toml \
    Cargo.lock

  # Pick up anything the steps above modified on top of those. `git add -u` only touches
  # tracked files that are already modified, and the pre-flight at the top of this script
  # guaranteed the working tree was clean before we started, so the only modifications that
  # exist now are the version bumps above, the refreshed visual baselines, plus oxfmt's
  # auto-fixes. This keeps the release commit in sync with what oxfmt --ci will see in CI.
  git add -u

  # Belt-and-braces: confirm the staged tree passes oxfmt in CI mode (no auto-fix). If this
  # fails, the release commit would carry formatting drift that CI rejects, so abort
  # instead of committing.
  pnpm check oxfmt --ci

  # Release gate: a stale translation must NOT ship. The i18n-stale check is warn-only in
  # normal `pnpm check` (a maintenance signal, not a daily-dev build breaker), but at release
  # we escalate it to a build-failing ERROR via CMDR_I18N_STALE_STRICT. With `set -e`, a stale
  # finding aborts the release before we tag, so the fix lands first. (English-only today, so
  # this is a clean no-op until a real locale exists.) See docs/guides/releasing.md.
  CMDR_I18N_STALE_STRICT=1 pnpm check i18n-stale -m

  # The staged release commit moves onto its own branch; local `main` stays a mirror.
  git switch -c "$RELEASE_BRANCH"
  git commit -m "$RELEASE_SUBJECT"
}

# Re-running after a stop picks up where it stopped: a release commit on its branch (CI went red
# and a re-run went green, a closed laptop) is landed, and a landed one is tagged.
BRANCH=$(git branch --show-current)
SUBJECT=$(git log -1 --format=%s)
if [[ "$BRANCH" == "$RELEASE_BRANCH" && "$SUBJECT" == "$RELEASE_SUBJECT" ]]; then
  echo "Resuming v$VERSION: the release commit is on $RELEASE_BRANCH; landing it."
elif [[ "$BRANCH" == main && "$SUBJECT" == "$RELEASE_SUBJECT" ]]; then
  echo "Resuming v$VERSION: the release commit is on main already; tagging it."
else
  prepare_release_commit
fi

if [[ "$(git branch --show-current)" == "$RELEASE_BRANCH" ]]; then
  # Push, PR, wait for `CI OK` on the release commit (25–35 min), then fast-forward `main` to it and
  # sync local `main`. `-rebase`: if `main` moved meanwhile, land it on top; the gate below then
  # wants a full run on the rebased release commit itself. A red run stops here: re-run its failed
  # jobs (or fix the commit), then re-run this script.
  pnpm land -rebase
  git switch main
  if [[ "$(git log -1 --format=%s)" != "$RELEASE_SUBJECT" ]]; then
    echo "Error: main isn't at the landed release commit. Sync it (git pull --ff-only origin main), then re-run."
    exit 1
  fi
  git branch -d "$RELEASE_BRANCH"
fi

# Whatever shipped from must have a green full run: the release commit's parent (the run above),
# or the release commit itself when `land` rebased it. A no-op in the usual case.
./scripts/release-ci-gate.sh

# A signed annotated tag proves the release trigger came from the trusted key whose public
# half lives in `.github/release-signers`. The workflow verifies it before starting a build.
# `-s` follows the user's configured signing format and key; any missing key or denied
# passphrase prompt aborts here rather than leaving an unsigned release tag behind.
if [[ "$(git rev-parse -q --verify "refs/tags/v$VERSION^{commit}" || true)" != "$(git rev-parse HEAD)" ]]; then
  git tag -s -m "Cmdr v$VERSION" "v$VERSION"
fi

echo ""
echo "Release v$VERSION is on main and tagged."
echo "To publish, run: git push origin v$VERSION"
# With RELEASE_UPDATE_SIGNING=local, the tag push only builds a draft; this laptop signs the update
# archives and has CI publish. See docs/guides/releasing.md § Who signs the update archives.
if [[ "$(gh variable get RELEASE_UPDATE_SIGNING -R vdavid/cmdr 2>/dev/null || true)" == "local" ]]; then
  echo "Then sign and publish it from this laptop: ./scripts/release-finish.sh $VERSION"
fi
