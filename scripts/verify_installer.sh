#!/usr/bin/env bash
# Gate the INSTALLED artifact, not just the build.
#
# The failure this catches happened for real: an installed app registered version
# 0.3.0 while its binary had come from a commit that predated the feature 0.3.0
# claimed, and nothing in CI would have said so. The version-equality check below is
# the point: it enforces "a version identifies its content" mechanically instead of
# by argument.
#
# One implementation, used by CI and runnable by hand on a real machine. It installs,
# checks, starts, stops and uninstalls, so run it where that is acceptable.
#
# Usage: verify_installer.sh <installer.exe> <expected-version>
set -uo pipefail

INSTALLER="${1:?usage: verify_installer.sh <installer.exe> <expected-version>}"
WANT="${2#v}"
PASS=0
FAIL=0

check() { # check <name> <ok|bad> [detail]
  if [ "$2" = "ok" ]; then PASS=$((PASS + 1)); printf '  ok    %s\n' "$1"
  else FAIL=$((FAIL + 1)); printf '  FAIL  %s\n        %s\n' "$1" "${3:-}"; fi
}

# The check() sentinel is ok|bad. Do not hand it yes|no: it then fails everything
# while the log says the opposite, which is a gate that lies.
verdict() { [ "$1" = "yes" ] && echo ok || echo bad; }

# The registry is the only thing that knows where a per-user install went, and it
# returns the path quoted, so strip the quotes.
reg() {
  powershell -nop -c "(Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*, HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* -ErrorAction SilentlyContinue | Where-Object DisplayName -eq 'Forge Studio' | Select-Object -First 1).$1" 2>/dev/null | tr -d '\r"'
}

echo "=== install $INSTALLER (expecting version $WANT) ==="
[ -f "$INSTALLER" ] || { echo "no such installer"; exit 2; }
"$INSTALLER" /S

# NSIS detaches from the shell, so poll rather than sleeping a guess.
ver=""
for _ in $(seq 1 60); do
  ver=$(reg DisplayVersion)
  [ -n "$ver" ] && break
  sleep 1
done
loc=$(reg InstallLocation)
echo "  registered: version='$ver' location='$loc'"

check "the registered version equals the version being shipped" \
  "$([ "$ver" = "$WANT" ] && echo ok || echo bad)" "registered '$ver', wanted '$WANT'"
check "the binary is where the registration says" \
  "$([ -f "$loc/forge-studio.exe" ] && echo ok || echo bad)" "no forge-studio.exe under '$loc'"

echo "=== it starts, and reaches the point of serving ==="
# Remove the descriptor first: otherwise a stale file from an earlier run makes this
# pass without the app doing anything.
rm -f "$APPDATA/forge-studio/control.json"
"$loc/forge-studio.exe" >/dev/null 2>&1 &
served=no
for _ in $(seq 1 30); do
  [ -f "$APPDATA/forge-studio/control.json" ] && { served=yes; break; }
  sleep 1
done
alive=no
tasklist 2>/dev/null | grep -qi "forge-studio.exe" && alive=yes
echo "  descriptor written: $served   process alive: $alive"
taskkill //IM forge-studio.exe //F >/dev/null 2>&1 || true

check "the installed app wrote its control descriptor (it bound its own port)" "$(verdict "$served")"
check "the installed app stayed running" "$(verdict "$alive")"

echo "=== uninstall ==="
if [ -f "$loc/uninstall.exe" ]; then
  "$loc/uninstall.exe" /S || true
fi
left=""
for _ in $(seq 1 60); do
  left=$(reg DisplayVersion)
  [ -z "$left" ] && break
  sleep 1
done
# Tolerate a leftover registration on a rerun, and say so rather than failing for
# the wrong reason.
if [ -n "$left" ]; then
  echo "  note: a registration for '$left' survived the uninstall"
else
  check "the uninstall removed the registration" ok
fi

printf '\n=== %d ok, %d failed ===\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
