#!/usr/bin/env bash
# Writes the build config for one haxny fork build: `build.conf.json` (the fork's identity) plus
# the fork version in the bundle, and a copy of the app's Info.plist that names the app "Cmdr haxny"
# in the menu bar and shows the fork version in Finder.
#
# Usage: .github/haxny/prepare-build.sh <fork-version> <out-config.json>
#   e.g. .github/haxny/prepare-build.sh 0.57.0-haxny.12 "$RUNNER_TEMP/haxny.conf.json"
# Then:  (cd apps/desktop && pnpm tauri build --bundles app,dmg --config <out-config.json>)
#
# The app's own semver (`tauri.conf.json` § version, `CARGO_PKG_VERSION`) stays upstream's: What's
# new, the security advisories, and crash reports compare it as plain x.y.z. Only the bundle's
# CFBundleShortVersionString / CFBundleVersion carry the `-haxny.<n>` suffix.
#
# Leaves `apps/desktop/src-tauri/Info.haxny.plist` behind (untracked); delete it after a local build.
set -euo pipefail

if [ $# -ne 2 ]; then
    echo "usage: $0 <fork-version> <out-config.json>" >&2
    exit 2
fi
version=$1
out=$2
case $version in
    *[!0-9A-Za-z.-]* | "") echo "bad fork version: $version" >&2; exit 2 ;;
esac

here=$(cd -- "$(dirname -- "$0")" && pwd)
repo=$(cd -- "$here/../.." && pwd)
tauri_dir=$repo/apps/desktop/src-tauri
plist=$tauri_dir/Info.haxny.plist
product=$(jq -r .productName "$here/build.conf.json")

# Sets a string key whether or not the copied plist already has it.
put() {
    /usr/libexec/PlistBuddy -c "Set :$1 $2" "$plist" 2>/dev/null ||
        /usr/libexec/PlistBuddy -c "Add :$1 string $2" "$plist"
}

cp "$tauri_dir/Info.plist" "$plist"
put CFBundleName "$product"
put CFBundleDisplayName "$product"
put CFBundleShortVersionString "$version"
put CFBundleVersion "$version"
plutil -lint "$plist" >/dev/null

jq --arg v "$version" '.bundle.macOS.bundleVersion = $v | .bundle.macOS.infoPlist = "Info.haxny.plist"' \
    "$here/build.conf.json" > "$out"
echo "Wrote $out and $plist for $product $version"
