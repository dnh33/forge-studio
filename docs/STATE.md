# State — resume here

Written so a cold start loses nothing: if the conversation is gone, this file plus
the ADRs and `git log` are enough to continue. Update it whenever the truth changes.

Last updated: 2026-10-08, during the `v0.3.1` build.

## What this is

One of three repositories that together form the **forge** ecosystem: a free,
public pipeline that renders images on GitHub's runners, and a desktop studio that
drives it. The studio is also an agent surface: a loopback HTTP control plane and
an MCP server, so a script or an agent can do everything the window can.

## The three repositories

| repo | what it is | state |
|---|---|---|
| `dnh33/forge-images` | **PUBLIC.** The render pipeline: sharded GitHub Actions jobs running FLUX.1-schnell Q4_K_S via `stable-diffusion.cpp` on `ubuntu-latest` CPU runners. Publishes to the `renders` branch. | working, benchmarked |
| `dnh33/forge-studio` | The Tauri v2 desktop app + `site/` landing page on GitHub Pages. Composer, run monitor, triage ledger, preview display, auto-updater, control plane, MCP server. | `v0.3.1` building |
| `dnh33/forge-motion` | LTX-Video image→video, local GPU only. **EXPERIMENTAL** — see the GPU warning below. | scaffolded, 34 tests |

**Visibility matters and is a live decision.** All three are public, so every
prompt in `prompts/*.json`, every image on the `renders` branch, and every run log
is world-readable. GitHub Free gives public repos unlimited Action minutes and
private repos 2,000/month — at ~45 min per image that is roughly 40 images a month
before private starts costing money. Danie has not chosen yet. Do not assume.

## Version state

- **`v0.2.0` — the only published release.**
- `v0.2.1` — draft, verified, superseded.
- `v0.3.0` — draft. **The installed app comes from this tag as it was at commit
  `57ddb9e`, which predates the preview reader.** The tag has since moved, which is
  why it must not be built again.
- **`v0.3.1` — current.** Adds: preview display (reads run artifacts), the three
  agent endpoints, `validate_decision`, rustfmt-canonical sources.

Rule learned the hard way: **never move a tag somebody has installed.** Bump the
version instead, because a version has to identify its content.

## Verified numbers — measured on runners, not estimated

| run | settings | wall clock |
|---|---|---|
| full render | 768x1024, 4 steps, FLUX.1-schnell Q4_K_S | **2699 s** (44 m 59 s) |
| preview | 192x256, 2 steps, same model | **220 s** (3 m 40 s) |
| Z-Image-Turbo, for comparison | 768x1024, 8 steps | 4689 s (78 m 09 s) |

Z-Image-Turbo is faster per step (586 s vs 675 s) but needs 8 steps, so Schnell
stays the default. Full detail in `docs/MODELS.md` in `forge-images`.

## Gates

Each repository's CI must be green before a tag. In `forge-studio` the `check` job
runs **`cargo fmt --check`** and **`cargo clippy --all-targets -- -D warnings`**,
then the tests. Two consequences worth remembering:

- Test modules must be the **last** item in a file — clippy rejects "items after a
  test module", and rustfmt-canonical does not imply clippy-clean.
- This PC has no Rust, so `cargo fmt` cannot be run locally. `.github/workflows/fmt.yml`
  produces rustfmt-canonical sources as an artifact for exactly this reason. Run it,
  download `formatted-src`, copy over `app/src-tauri/src/`, and compare with line
  endings normalised before believing a diff.

## Test inventory

| repo | tests |
|---|---|
| `forge-images` | 59 pytest |
| `forge-studio` | 8 Rust + 12 MCP (Node) + 14 Python (`scripts/`) |
| `forge-motion` | 34 pytest |

## Where the app is installed

`E:\Forge Studio\forge-studio.exe` (the user chose E:, not the default). Its
registry entry reports `DisplayVersion`. On launch it writes the control-plane
descriptor to `%APPDATA%\forge-studio\control.json` — `{name, pid, port, token, url}` —
plus `control-token` and a rotating `control.log`. Port 7317.

`scripts/live_smoke.sh <installer.exe> [run_id] [preview_run_id]` drives an installed
build end to end: installs silently, launches, reads the descriptor, proves auth is
enforced, exercises every endpoint, refuses a bad verdict, and checks the MCP
server. This is the only honest test of a GUI app whose window cannot be clicked.

## GPU warning (do not forget this)

Local GPU rendering on the RTX 5060 Ti saturated 15.9 GB of 16.3 GB and **froze the
desktop**. Local GPU work is EXPERIMENTAL and must never be run unprompted. Cloud CI
is the default. See `forge-motion`.

## Decisions taken (see `docs/adr/`)

1. **ADR-0001 — engine/UI split.** APPROVED 2026-10-08. The engine becomes headless
   and the window becomes a viewport.
2. **ADR-0002 — installer smoke test in CI.** APPROVED 2026-10-08.
3. **ADR-0003 — one-click onboarding and the account lock.** APPROVED 2026-10-08,
   design in the ADR.

## In flight at the time of writing

- The `v0.3.1` release build (all three platforms).
- Render run `37770433177` — Danie's own dispatch, six shards, publishing to `renders`.
- The live smoke test of `0.3.1` on this PC, once the build lands.

## Next actions

1. When the `v0.3.1` build finishes: verify all three platforms report their
   signature counts, put `Forge.Studio_0.3.1_x64-setup.exe` on the Desktop, and run
   `scripts/live_smoke.sh` with the preview run id `37768167306` and the published run
   id once Danie's render lands.
2. ADR-0002: add the installer smoke job to `release.yml` (`windows-latest`).
3. ADR-0001: write the engine/UI split spec before writing the code.
4. ADR-0003: implement onboarding (create-from-template) and the actor gate.
5. Ask Danie the visibility question directly: public and free, or private and
   metered.
6. **Audit the test suites against the "only where failure is costly" rule.** Some
   suites were written after the fact to reach a count, which is exactly the habit
   that rule forbids. Keep the guards that caught real bugs (preview publish
   exclusion, the `only` filter, path escapes, the redirect token, the decision
   vocabulary, IPC naming); propose dropping the ones that only restate
   implementation. Ask before deleting anything.
