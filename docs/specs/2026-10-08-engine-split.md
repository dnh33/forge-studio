# Spec — engine/UI split

Status: **ready to implement.** Authority: ADR-0001 (approved 2026-10-08).
Written before the code, as that ADR requires.

## Goal

One binary, headed or headless. The **engine** — the control plane, GitHub access,
the run watch loop, the ledger — runs with no window at all. The **window** is a
viewport onto an engine.

Today the control plane lives inside the GUI process, so closing the window kills
the live run monitor and any agent driving the studio. That is the defect.

## In scope for this cut

1. **`--headless` starts the engine and no window.** Descriptor written, control
   plane listening, process stays alive, no webview created.
2. **Engine discovery.** On launch, before starting its own engine, the app probes
   the descriptor's port. If an engine answers as ours, attach to it instead of
   starting a second one.
3. **Single engine per user.** Two engines must never fight over the port.
4. **The window behaves exactly as it does today** when it owns the engine. No
   behaviour change for the normal path.

## Deliberately out of scope

- A Windows service / launchd install. A headless mode gets most of the value; a
  service can follow if it earns its complexity.
- Multiple users, multiple machines, or remote access. The engine stays loopback.
- A second RPC surface for UI state. The window keeps calling Tauri commands
  in-process when it owns the engine; the HTTP contract stays as it is.

## The contract

- **`docs/CONTROL.md` is the engine's contract.** No new endpoints for this cut.
- The descriptor stays `%APPDATA%\forge-studio\control.json` with
  `{name, pid, port, token, url}`. Add `"role": "engine"` so a viewport can tell
  what it found. An older descriptor without the field is still valid.
- The engine must require **no display, no webview, no window server**. If it needs a
  window to run, this cut has failed.

## Acceptance — what "done" means, mechanically

Each of these is runnable. A claim against any of them needs the command and its
output, not a description.

| # | Check | How |
|---|---|---|
| 1 | Headless starts an engine and no window | run `forge-studio.exe --headless`; descriptor appears; `GET /status` returns 200 with the token; process still alive after 30 s |
| 2 | No second engine | launch the GUI while headless runs; the descriptor's `pid` is **unchanged** afterwards |
| 3 | The engine outlives the window | kill the GUI process; `GET /status` still answers |
| 4 | Auth is still enforced | `GET /status` without a token returns 401, headless exactly as headed |
| 5 | The existing suite still passes | `scripts/live_smoke.sh` against the headless engine: 12 ok, 0 failed |
| 6 | A stale descriptor does not wedge startup | kill the engine, leave the descriptor, launch again: a new engine starts and rewrites it |

## Risks, named

- **Discovery must be a real request, not a file existence check.** A stale
  descriptor is normal (the engine died). Probing must connect and ask, and on
  refusal take the port.
- **The port may be held by something else entirely.** A non-ours response on 7317
  must not be mistaken for an engine.
- **Windows console flashing.** Any child process spawned by the engine needs
  `CREATE_NO_WINDOW` (already the pattern in `github.rs`).
- **A viewport that attaches must not silently lose the ability to act.** If the
  engine owns the data and the window calls a command, the window's call must reach
  the engine's state, or the split has forked the state in two.

## Why this shape and not a separate binary

A second binary means a second thing to version, sign, install and explain, and two
things that can disagree about the protocol. One binary with a flag gets the
behaviour with none of that. If a service install is ever wanted, it installs *this*
binary with `--headless`.
