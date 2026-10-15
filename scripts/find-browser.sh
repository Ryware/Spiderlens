#!/usr/bin/env bash
# Lists the Chromium-family browsers installed in the standard places on this
# machine (spec 014, task T002), one per line: name, path and version,
# separated by tabs. Runs on Linux, macOS and Windows (Git Bash).
#
#   bash scripts/find-browser.sh
#
# $SPIDERLENS_BROWSER, if set, is checked first. Exits 0 when at least one
# browser was found and 1 otherwise. Only reads; never downloads anything.

set -u

found=0
seen=""

version_of() {
  local path="$1"
  case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*)
      # chrome.exe and msedge.exe print nothing for --version on Windows;
      # the version is in the file's version resource.
      local win
      win="$(cygpath -w "$path" 2>/dev/null || printf '%s' "$path")"
      powershell.exe -NoProfile -Command "(Get-Item -LiteralPath '$win').VersionInfo.ProductVersion" 2>/dev/null |
        tr -d '\r'
      ;;
    *)
      "$path" --version 2>/dev/null | head -n 1
      ;;
  esac
}

check() {
  local name="$1" path="$2"
  [ -n "$path" ] && [ -f "$path" ] && [ -x "$path" ] || return 0
  # The same binary is often reachable under several names.
  local real
  real="$(readlink -f "$path" 2>/dev/null || printf '%s' "$path")"
  case "$seen" in *"|$real|"*) return 0 ;; esac
  seen="$seen|$real|"
  local version
  version="$(version_of "$path")"
  printf '%s\t%s\t%s\n' "$name" "$path" "${version:-unknown version}"
  found=$((found + 1))
}

check_command() {
  local name="$1" cmd="$2" path
  path="$(command -v "$cmd" 2>/dev/null)" || return 0
  check "$name" "$path"
}

if [ -n "${SPIDERLENS_BROWSER:-}" ]; then
  check "SPIDERLENS_BROWSER" "$SPIDERLENS_BROWSER"
fi

case "$(uname -s)" in
  Linux)
    check_command "Google Chrome" google-chrome-stable
    check_command "Google Chrome" google-chrome
    check "Google Chrome" /opt/google/chrome/chrome
    check_command "Chromium" chromium
    check_command "Chromium" chromium-browser
    check "Chromium" /snap/bin/chromium
    check_command "Microsoft Edge" microsoft-edge-stable
    check_command "Microsoft Edge" microsoft-edge
    check "Microsoft Edge" /opt/microsoft/msedge/msedge
    ;;
  Darwin)
    for root in /Applications "$HOME/Applications"; do
      check "Google Chrome" "$root/Google Chrome.app/Contents/MacOS/Google Chrome"
      check "Chromium" "$root/Chromium.app/Contents/MacOS/Chromium"
      check "Microsoft Edge" "$root/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"
    done
    ;;
  MINGW* | MSYS* | CYGWIN*)
    pf="$(printenv ProgramFiles 2>/dev/null || true)"
    pf86="$(printenv 'ProgramFiles(x86)' 2>/dev/null || true)"
    local_app="$(printenv LOCALAPPDATA 2>/dev/null || true)"
    for root in "$pf" "$pf86" "$local_app"; do
      [ -n "$root" ] || continue
      root="$(cygpath -u "$root" 2>/dev/null || printf '%s' "$root")"
      check "Google Chrome" "$root/Google/Chrome/Application/chrome.exe"
      check "Chromium" "$root/Chromium/Application/chrome.exe"
      check "Microsoft Edge" "$root/Microsoft/Edge/Application/msedge.exe"
    done
    ;;
esac

if [ "$found" -eq 0 ]; then
  echo "No Chrome, Chromium or Edge found in the standard locations." >&2
  exit 1
fi
