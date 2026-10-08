# ADR-0001 — Split the engine from the window

Status: **APPROVED** 2026-10-08. Not yet designed in detail; do not write code before the spec.

## Context

The studio's control plane, its run monitor and its MCP server all live inside the
GUI process. Close the window and all three stop. That makes two features
impossible rather than merely awkward:

- "Watch the pipeline live" cannot survive closing the window, which is precisely
  when a 45-minute render is worth watching.
- An agent driving the studio needs it to be reachable while nobody is looking at
  a window.

Measured today: the control plane writes its descriptor on launch
(`%APPDATA%\forge-studio\control.json`, port 7317) and dies with the process. The
app is, in the useful sense, a window rather than a service.

The prompt for this came from Zeron (`zeronsh/zeron`, MIT), whose architecture doc
describes an **engine** that is "pure Rust, fully functional headless" and a **UI**
that is a viewport talking the same typed RPC whether the engine is in-process or a
separate daemon, shipped as one binary that is headed or headless. We take the idea,
not the code: ideas and architectures are not copyrightable, their source is, and we
write our own either way.

## Decision

Split the app into an **engine** and a **viewport**, in the same binary:

- The engine owns the control plane, GitHub access, the watch loop and the ledger.
  It is fully functional with no window.
- The viewport renders engine state and talks to it over the same typed RPC the
  control plane already speaks.
- One binary: started with a window it does both; started headless it is only the
  engine. If an engine is already listening, a second launch is a viewport onto it.

## Alternatives considered

- **Keep it embedded.** Rejected: the two features above stay impossible, and every
  agent integration is tied to a human having a window open.
- **A separate daemon binary with a service install.** Considered and deferred. It
  buys always-on behaviour across reboots, but costs a service lifecycle, an
  installer change, elevation questions and a second thing to version. The same
  binary with a headless mode gets most of the value first; a service can follow.
- **A hosted backend.** Rejected: the app is local-first and free, and the pipeline
  is already a hosted backend. A second one adds cost and a privacy surface.

## Consequences

- The studio can keep watching a render with the window closed, and an agent can
  drive it with no window at all.
- New coordination problems appear and must be designed, not discovered: single
  instance, who owns the port, what happens if the engine dies while a viewport is
  attached, and how the viewport finds the engine.
- The control plane's contract becomes the engine's contract, so `docs/CONTROL.md`
  is the interface to design against rather than an add-on.
