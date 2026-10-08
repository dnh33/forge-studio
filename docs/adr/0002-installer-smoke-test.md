# ADR-0002 — Test the installer in CI, not by hand

Status: **APPROVED** 2026-10-08.

## Context

Today the installed app was found at `E:\Forge Studio`, its registry entry reading
`0.3.0`, built from commit `57ddb9e` — which predates the preview reader. Nothing
in our CI would have caught that the shipped build did not match the tag, and the
confusion was only resolved by hand: reading the registry, grepping the commit for
a symbol, and installing it again.

Zeron ships `scripts/test-windows-installer.ps1`, which installs, inspects and
uninstalls the built installer silently, and notes that it touches the current
user's registration so it belongs in CI or behind a `-Force`. That is the gap and
the shape of the fix.

## Decision

Add a job to `release.yml` on `windows-latest` that, for the installer this release
just built:

1. runs it silently (`/S`),
2. reads back the installed version from the registry and **fails unless it equals
   the tag being released**,
3. confirms the binary exists and launches to the point of writing its control
   descriptor,
4. uninstalls, and confirms the registration is gone.

The version equality check is the point. It is the assertion that a version
identifies its content, enforced mechanically instead of by an argument.

## Alternatives considered

- **Test the unpacked binary instead of the installer.** Rejected: it tests the
  binary, and the failures we actually hit live in the packaging and the
  registration.
- **Verify the updater's `latest.json` only.** Already done in `verify_update.py`.
  Necessary, not sufficient: it says nothing about whether the installer installs.
- **Rely on manual checks.** That is what failed today.

## Consequences

- A mismatched or broken installer fails the release instead of reaching the
  Desktop.
- A few minutes of runner time per release.
- The job writes to the user profile of a throwaway runner, never to Danie's
  machine. Per-user installs need no elevation, so this works unattended.
- The uninstall step must tolerate a missing registration on a rerun, or reruns
  will fail for the wrong reason.

## Implemented, 2026-10-08

`scripts/verify_installer.sh` holds the assertions, and the `installer` job in
`release.yml` runs it against the installer the release just built. One
implementation, so CI and a human on a real machine exercise the same code rather
than two that drift.

Verified **both ways** before trusting it, which is the only way to know a gate can
fail at all:

| case | result |
|---|---|
| expecting `0.3.0` against a `0.3.1` installer | exit **1**, version check FAILED |
| expecting `0.3.1` | exit **0**, 5/5 ok |
| app reinstalled afterwards | still installed at `E:\Forge Studio`, `0.3.1` |

It also caught two bugs in the script's own first draft: a `check()` that took an
`ok`/`bad` sentinel while being handed `yes`/`no` (so it failed everything while the
log said the opposite), and an exit code measured through a pipe, which reports the
pipe's status rather than the script's.

**Not yet exercised:** the CI wiring itself. The job runs for the first time on the
next release, because rebuilding `v0.3.1` to test it would have moved a tag Danie has
installed, which is the exact fault this ADR exists to prevent.
