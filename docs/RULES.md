# Rules for anyone (or anything) working in this repository

An `AGENTS.md` at the repository root would be the conventional home for this, but
that path is a protected agent-instruction file and the write needs Danie's explicit
approval, so the rules live here until he grants it.

Read this, then `docs/STATE.md`, then `docs/CONTROL.md`.

## What this repository is

The desktop studio and web presence for the **forge** pipeline. A Tauri v2 app in
Rust plus plain JavaScript — no TypeScript, no bundler, no framework — a landing
page in `site/`, an MCP server in `mcp/`, tests in `tests/`.

It is not only a GUI. While it runs it exposes a loopback HTTP control plane and
ships an MCP server on top of it, so an agent can do everything the window can.
`docs/CONTROL.md` is that contract.

## Rules

1. **Never claim done on a masked build.** No `ignoreBuildErrors`, no skipped
   type-check, no "it compiles" without the gate having run. Check, test, build,
   then say it works.
2. **The gates are `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`,
   then the tests.** Format before pushing or `check` is red. Test modules go
   **last** in a file: clippy rejects "items after a test module", and being
   rustfmt-canonical does not make you clippy-clean. This PC has no Rust toolchain,
   so run `.github/workflows/fmt.yml` and copy its artifact in.
3. **A version must identify its content.** Never move or rebuild a tag anyone has
   installed — bump instead. `v0.3.0` is the cautionary tale: it was moved, and the
   installed build stopped matching the tag.
4. **Never let a test mutate the repository.** `gh` is preinstalled on GitHub
   runners and authenticated in some environments, so a test that calls a writing
   function can commit to a real repo. Validation is a pure function
   (`validate_decision`) so it can be tested in isolation, and every test of a write
   path must be able to prove it touches no network.
5. **A token must never follow a redirect.** GitHub serves artifact zips from a
   storage host after a redirect. Resolve redirects by hand and attach the bearer
   token only where it belongs.
6. **Secrets live in the OS keyring or the user's environment** — never in the
   webview, never in a file in this repository, never in `localStorage`.
7. **No AI slop in copy.** No em-dashes or en-dashes. No "seamless", "empower",
   "next-gen", "revolutionize". Concrete and specific beats impressive.
8. **Local GPU work is EXPERIMENTAL.** It saturated VRAM and froze the desktop once.
   Cloud CI is the default and local GPU work is never run unprompted.
9. **A claim needs evidence, and a number needs computing.** Never quote a figure
   that was not measured, and when something is verified say by what.
10. **Write tests only where failure is costly.** Not coverage for its own sake. A
    test earns its place by encoding a real requirement or guarding an invariant
    whose breach would hurt, and by being a feedback loop someone actually trusts.
    This repository's keepers are the ones that caught real bugs: previews never
    publish, the `only` filter must not drop ad-hoc items, a path must not escape a
    route, the token must not follow a redirect, the decision vocabulary is closed,
    and JS/Rust IPC argument names must agree.

    The failure mode to refuse: an agent writing tests that restate its own
    implementation, then treating "my tests pass" as completion. That proves nothing
    about the requirement, because the same mind wrote both sides. Tests written
    after the fact to reach a number are the same mistake wearing a green tick.

## Commands

```bash
python3 -m pytest tests -q                           # Python tests
node --test mcp/forge-studio-mcp.test.mjs            # MCP tests
gh workflow run fmt.yml -R dnh33/forge-studio        # canonical Rust sources
gh run download <id> -R dnh33/forge-studio -n formatted-src
scripts/live_smoke.sh <installer.exe> [run_id] [preview_run_id]
```

Tauri exposes a Rust command's parameters to JavaScript in **camelCase**: Rust
`run_id` is `runId` in `invoke`. Getting this wrong fails with "missing required key
runId". `tests/test_ipc_contract.py` guards it.
