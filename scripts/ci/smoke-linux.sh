#!/usr/bin/env bash
# Smoke test of the debug app on Linux, under xvfb and a private D-Bus, with a
# Markdown document to open: the app must stay up for a while, without a panic.
set -euo pipefail

seconds=20
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

cargo build -p papers-write

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# xvfb-run merges the error output of the app into its output
log=$work/app.log
printf -- '---\ntitle: Smoke\n---\n\n# Intro {#sec-intro}\n\nSee @sec-intro.\n' > "$work/paper.md"

# a temporary home keeps the settings of the runner (and of a developer) out
home=$work/home
mkdir -p "$home/.config"

# the app runs until the timeout stops it (124): a crash ends it earlier
status=0
env HOME="$home" XDG_CONFIG_HOME="$home/.config" WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  dbus-run-session -- xvfb-run -a timeout "$seconds" target.noindex/debug/papers-write "$work/paper.md" \
  > "$log" 2>&1 || status=$?

failures=()
[[ $status -eq 124 ]] || failures+=("exit status $status, expected 124 (still running after ${seconds}s)")
! grep -q panicked "$log" || failures+=("the app panicked")

if ((${#failures[@]})); then
  printf 'smoke test failed: %s\n' "${failures[@]}" >&2
  echo "--- output of the app ---" >&2
  cat "$log" >&2
  exit 1
fi
echo "smoke test passed: the app ran for ${seconds}s"
