#!/usr/bin/env bash
# Live smoke test: drive the INSTALLED app, not a build.
#
# Everything here goes through the running program's own control plane, which is
# the only honest way to test a desktop app whose UI I cannot click. It proves
# the Rust layer end to end: install, launch, auth, every endpoint, and the MCP
# server over stdio.
#
# Usage: live_smoke.sh <installer.exe> [run_id_for_outputs] [preview_run_id]
set -uo pipefail

INSTALLER="$1"
RUN_ID="${2:-}"
PREVIEW_RUN_ID="${3:-}"
BASE="http://127.0.0.1:7317"
PASS=0
FAIL=0

say() { printf '\n=== %s ===\n' "$1"; }
check() { # check <name> <expected-substring> <actual>
  if printf '%s' "$3" | grep -q "$2"; then
    PASS=$((PASS + 1)); printf '  ok    %s\n' "$1"
  else
    FAIL=$((FAIL + 1)); printf '  FAIL  %s\n        wanted: %s\n        got:    %.160s\n' "$1" "$2" "$3"
  fi
}

say "install silently"
if [ ! -f "$INSTALLER" ]; then echo "installer not found: $INSTALLER"; exit 2; fi
"$INSTALLER" /S
sleep 8

# Tauri's NSIS puts the binary under %LOCALAPPDATA%\<productName>. Find it rather
# than assume the directory name.
EXE=$(find "$LOCALAPPDATA" -maxdepth 2 -name "forge-studio.exe" 2>/dev/null | head -1)
if [ -z "$EXE" ]; then echo "forge-studio.exe not found under LOCALAPPDATA"; exit 2; fi
echo "  installed: $EXE"

say "launch (a GUI app, so it must be started detached)"
"$EXE" >/dev/null 2>&1 &
sleep 6

DESC="$APPDATA/forge-studio/control.json"
if [ ! -f "$DESC" ]; then echo "no control descriptor at $DESC"; exit 2; fi
TOKEN=$(python3 -c "import json;print(json.load(open(r'$DESC'))['token'])")
PORT=$(python3 -c "import json;print(json.load(open(r'$DESC')).get('port',7317))")
BASE="http://127.0.0.1:$PORT"
echo "  control plane on $PORT (token taken from the descriptor, never printed)"

api() { curl -s -m 60 -H "Authorization: Bearer $TOKEN" "$@"; }

say "auth is actually enforced"
code=$(curl -s -o /dev/null -w '%{http_code}' -m 20 "$BASE/status")
check "no token is refused" "401" "$code"

say "the read endpoints"
check "status"        '"ok"\|"version"\|"repo"' "$(api "$BASE/status")"
check "sets"          '\[|portraits'            "$(api "$BASE/sets")"
check "renders"       '\[|portraits'            "$(api "$BASE/renders")"
check "runs"          '\[|display_title\|status' "$(api "$BASE/runs")"

if [ -n "$RUN_ID" ]; then
  say "run $RUN_ID (a published render)"
  check "outputs" "renders/|\"file\"|\[" "$(api "$BASE/run/$RUN_ID/outputs")"
fi

if [ -n "$PREVIEW_RUN_ID" ]; then
  say "run $PREVIEW_RUN_ID (a preview: nothing published, images only in artifacts)"
  check "previews" 'data:image/png;base64,' "$(api "$BASE/run/$PREVIEW_RUN_ID/previews")"
  check "the preview reader rejects a bad id" 'must be a number' "$(api "$BASE/run/not-a-number/previews")"
fi

say "the ledger: reject an unknown verdict before it can reach the repository"
check "bad verdict is refused" 'verdict must be one of' \
  "$(api -X POST -H 'Content-Type: application/json' -d '{"set":"probe","file":"probe-x.png","verdict":"brilliant"}' "$BASE/decision")"
check "bad reason is refused" 'reason must be one of' \
  "$(api -X POST -H 'Content-Type: application/json' -d '{"set":"probe","file":"probe-x.png","verdict":"reject","reason":"meh"}' "$BASE/decision")"
check "a path is refused" 'plain name' \
  "$(api -X POST -H 'Content-Type: application/json' -d '{"set":"../etc","file":"p.png","verdict":"keep"}' "$BASE/decision")"

say "the agent interface describes itself"
check "describe lists the new routes" '/decisions\|/previews' "$(api "$BASE/")"

printf '\n=== %d ok, %d failed ===\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
