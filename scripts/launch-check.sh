#!/usr/bin/env bash
# Launch check (FR-027, US3-4, US4-1): starts the built CLI and the built
# desktop package on this runner's OS and checks that both report the same
# build identity, and that it is the expected build id.
#
# Usage: scripts/launch-check.sh <rust-target> <expected-build-id>
set -euo pipefail

target="$1"
expected="$2"
release="target/$target/release"
out="$(pwd)/launch-check"
mkdir -p "$out"

case "$target" in
  *windows*) cli="$release/spiderlens.exe" ;;
  *) cli="$release/spiderlens" ;;
esac

echo "== CLI: $cli"
"$cli" version --json > "$out/cli.json"
cat "$out/cli.json"

export SPIDERLENS_SMOKE_TEST="$out/desktop.json"
rm -f "$SPIDERLENS_SMOKE_TEST"

case "$target" in
  *linux*)
    appimage=$(ls "$release"/bundle/appimage/*.AppImage)
    chmod +x "$appimage"
    echo "== Desktop: $appimage"
    # CI runners have no FUSE; the runtime can extract itself instead.
    APPIMAGE_EXTRACT_AND_RUN=1 xvfb-run -a "$appimage"
    ;;
  *apple*)
    dmg=$(ls "$release"/bundle/dmg/*.dmg)
    mnt=$(mktemp -d)
    echo "== Desktop: $dmg"
    hdiutil attach -nobrowse -readonly -mountpoint "$mnt" "$dmg" > /dev/null
    "$mnt/SpiderLens.app/Contents/MacOS/spiderlens-desktop"
    hdiutil detach "$mnt" > /dev/null
    ;;
  *windows*)
    installer=$(ls "$release"/bundle/nsis/*-setup.exe)
    echo "== Desktop: $installer (silent per-user install)"
    powershell -NoProfile -Command "Start-Process -FilePath '$(cygpath -w "$installer")' -ArgumentList '/S' -Wait"
    "$LOCALAPPDATA/SpiderLens/spiderlens-desktop.exe"
    ;;
  *)
    echo "unknown target $target" >&2
    exit 1
    ;;
esac

python3 - "$out/cli.json" "$out/desktop.json" "$expected" <<'PY'
import json, sys
cli = json.load(open(sys.argv[1]))
desktop = json.load(open(sys.argv[2]))
expected = sys.argv[3]
print("desktop:", desktop)
assert cli == desktop, f"CLI and desktop disagree: {cli} != {desktop}"
assert cli["build_id"] == expected, f"build id {cli['build_id']} != expected {expected}"
print(f"launch check passed: build {expected}")
PY
