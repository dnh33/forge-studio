# Spec: make being wrong cheap

2026-10-08. Approved by owner. Ships as forge-studio `0.3.0` on top of `0.2.1`.

## Problem

One full render is 2699 s (measured twice). A correction therefore costs a night, so
the operator makes one or two decisions per session and quits. The bottleneck is
feedback latency, not model quality.

Measured basis (one dispatch, same runners, same prompt):

| canvas | steps | render | wall (incl. setup) |
|---|---|---|---|
| 256x256 | 1 | 123 s | ~6 min |
| 256x256 | 4 | 252 s | ~7 min |
| 768x1024 | 4 | 2699 s | 48 min |

1-step to 4-step costs 129 s for 3 steps, so denoising dominates at this canvas and
the fixed costs (model load, text encode) do not swamp a preview. A sharded set
previews in roughly the wall time of one shard.

## Scope

### 1. Preview mode (forge-images)

- `render.yml` gains `preview` (`true`/`false`, default `false`).
- `plan.py` gains `PREVIEW_SCALE` (default `0.25`) and `PREVIEW_STEPS` (default `2`).
  When previewing, each job's canvas is multiplied by the scale, kept to the same
  aspect, and rounded **down to a multiple of 16** (VAE constraint). Steps are
  clamped to `PREVIEW_STEPS`.
- Previews go to the `preview-<shard>` artifact name and are **never** written to the
  `renders` branch: the publish step filters on the artifact name.
- A preview run is marked `[preview]` so it cannot be mistaken for a real one.

### 2. Judgment ledger (forge-studio + renders branch)

- One file per render, `renders/<set>/<name>.decision.json` (so it commits beside the
  image and its sidecar, and is git-versioned).
- Schema: `{image, verdict: keep|reject|undecided, reason: <enum|null>, note: <str, <=200>, at: <iso>}`
- Reason enum (closed): `muddy`, `off-style`, `wrong-subject`, `wrong-composition`,
  `artifacts`, `duplicate`, `close-but-off`.
- Validation before write, exactly like `save_set`: unknown verdict or reason is a 400,
  never a silent write.
- The app gains a keyboard-driven triage pass over a run's outputs: arrows move, `k`/`r`
  decide, digits pick the reason, `u` clears. Writes are per-image and immediate.

### 3. Wire `or_sharpen` into targeted re-dispatch (forge-studio)

- `or_sharpen` exists in Rust and nothing calls it. Add the button on each item row of
  the composer.
- It shows a **diff** (old line vs new) and requires explicit accept. No silent rewrite.
- After a triage pass, `re-dispatch rejected` sends `only=<rejected item ids>` so a
  correction costs the fixed lines only.

## Out of scope (council-blocked)

- No autonomous loop: nothing dispatches without a human action.
- No aesthetic score presented as truth. Reason codes are the operator's; a model may
  only ever draft them.
- No local GPU. No new datastore. No new service.

## Verification

- `plan.py`: preview scale applies, aspect is preserved, canvas is a multiple of 16,
  steps clamp, and the job still carries a full-size-equivalent id. Unit tests.
- A test asserting a preview run cannot reach the publish step.
- Ledger: reject an unknown verdict/reason (400), accept a valid record, round-trip.
- IPC contract test already guards JS/Rust argument naming.
- One live preview dispatch to confirm wall time against this spec.
