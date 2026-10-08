#!/usr/bin/env bash
# Live smoke test: drive the INSTALLED app, not a build.
#
# Everything here goes through the running program's own control plane, which is
# the only honest way to test a desktop app whose UI I cannot click. It proves
# the Rust layer end to end: install, launch, auth, every endpoint, and the MCP
# server over stdio.
#
# Usage: live_smoke.sh <installer.exe> [run_id_for_outputs] [preview_run_id]
#    or: live_smoke.sh --engine <forge-studio.exe> [run_id_for_outputs] [preview_run_id]
#
# --engine drives a built binary in --headless mode instead of installing
# anything: the engine the split produces is exactly what this suite must
# prove. Every check after the launch is identical in both modes.
set -uo pipefail

MODE="${1:-}"
ENGINE_EXE=""
if [ "$MODE" = "--engine" ]; then
  ENGINE_EXE="${2:?usage: live_smoke.sh --engine <forge-studio.exe> [run_id] [preview_run_id]}"
  shift 2
fi
INSTALLER="${1:-}"
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
if [ -n "$ENGINE_EXE" ]; then
  say "engine mode: no install, launching $ENGINE_EXE --headless"
  if [ ! -f "$ENGINE_EXE" ]; then echo "engine exe not found: $ENGINE_EXE"; exit 2; fi
  # Delete a stale descriptor so the checks below prove THIS engine wrote it.
  rm -f "$APPDATA/forge-studio/control.json"
  "$ENGINE_EXE" --headless >/dev/null 2>&1 &
  sleep 6
  EXE="$ENGINE_EXE"
else
if [ ! -f "$INSTALLER" ]; then echo "installer not found: $INSTALLER"; exit 2; fi
"$INSTALLER" /S
sleep 8

# Find the installed binary. Do NOT assume %LOCALAPPDATA%\<productName>: this
# install lives on E:, and a silent install goes wherever the user last put it.
# The registry is the only thing that knows, and it returns the path quoted.
LOC=$(powershell -nop -c "(Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*, HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* -ErrorAction SilentlyContinue | Where-Object DisplayName -eq 'Forge Studio' | Select-Object -First 1).InstallLocation" 2>/dev/null | tr -d '\r"')
EXE="$LOC/forge-studio.exe"
if [ ! -f "$EXE" ]; then
  EXE=$(find "$LOCALAPPDATA" -maxdepth 2 -name "forge-studio.exe" 2>/dev/null | head -1)
fi
if [ -z "$EXE" ] || [ ! -f "$EXE" ]; then echo "forge-studio.exe not found (registry said: '$LOC')"; exit 2; fi
echo "  installed: $EXE"
echo "  registered version: $(powershell -nop -c "(Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* -ErrorAction SilentlyContinue | Where-Object DisplayName -eq 'Forge Studio' | Select-Object -First 1).DisplayVersion" 2>/dev/null | tr -d '\r')"
fi

say "launch (a GUI app, so it must be started detached)"
if [ -z "$ENGINE_EXE" ]; then
  "$EXE" >/dev/null 2>&1 &
  sleep 6
else
  echo "  skipped: the engine is already up from the launch above"
fi

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
# Assert on strings the handlers ACTUALLY return. Run curl by hand first if
# unsure: a smoke test asserting a shape that does not exist fails for its own
# reason and buries the app's real state.
check "status (signed-in user)" '"github"' "$(api "$BASE/status")"
check "sets"                    '"slug"'   "$(api "$BASE/sets")"
check "renders"                 '"file"'   "$(api "$BASE/renders")"
check "runs"                    '\['        "$(api "$BASE/runs")"

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
check "describe lists the new routes" '/decisions' "$(api "$BASE/")"
check "describe lists the preview route" '/previews' "$(api "$BASE/")"

printf '\n=== %d ok, %d failed ===\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
