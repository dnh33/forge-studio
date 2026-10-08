"""The JS <-> Rust command contract.

Tauri v2 renames a Rust command's parameters to camelCase for JavaScript. So
`fn run_outputs(run_id: u64)` must be called as `invoke("run_outputs", { runId })`,
and passing `run_id` fails at runtime with a message the user sees:

    invalid args `runId` for command `run_outputs`:
    command run_outputs missing required key runId

That shipped once. It is not a type error the compiler can catch, and it is not
reachable from a unit test, so this checks the two files against each other
statically instead: parse the command signatures out of lib.rs, parse every
`invoke(...)` / `call(...)` in app.js, and assert the argument names agree.
"""
import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
LIB = REPO / "app" / "src-tauri" / "src" / "lib.rs"
UI = REPO / "app" / "ui" / "app.js"

# Parameters Tauri injects rather than expects from JS.
INJECTED = {"app", "window", "state", "webview"}


def rust_commands():
    """Map command name -> set of JS-facing (camelCase) parameter names."""
    src = LIB.read_text(encoding="utf-8")
    commands = {}
    for m in re.finditer(
        r"#\[tauri::command\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(([^)]*)\)",
        src,
        re.S,
    ):
        name, params = m.group(1), m.group(2)
        js_names = set()
        for part in params.split(","):
            part = part.strip()
            if not part or ":" not in part:
                continue
            arg = part.split(":")[0].strip()
            if arg in INJECTED:
                continue
            head, *rest = arg.split("_")
            js_names.add(head + "".join(w.capitalize() for w in rest))
        commands[name] = js_names
    return commands


def js_calls():
    """Every (command, {keys}) pair the UI invokes, single-level objects only."""
    src = UI.read_text(encoding="utf-8")
    calls = []
    for m in re.finditer(r'\b(?:invoke|call)\(\s*"(\w+)"\s*(?:,\s*(\{[^{}]*\}))?', src):
        name, obj = m.group(1), m.group(2)
        keys = set()
        if obj:
            keys = set(re.findall(r"(\w+)\s*:", obj))
        calls.append((name, keys))
    return calls


def test_the_ui_and_the_rust_commands_were_both_found():
    commands = rust_commands()
    calls = js_calls()
    assert "run_outputs" in commands, "lib.rs parsed wrong: no run_outputs"
    assert any(n == "run_outputs" for n, _ in calls), "app.js parsed wrong"
    assert len(commands) > 15, f"only found {len(commands)} commands"


def test_every_argument_name_the_ui_passes_exists_on_the_command():
    commands = rust_commands()
    problems = []
    for name, keys in js_calls():
        if name not in commands:
            problems.append(f"{name}: called from JS but not a command in lib.rs")
            continue
        for key in keys - commands[name]:
            problems.append(
                f'{name}: JS passes "{key}", but the Rust parameters are '
                f"{sorted(commands[name]) or ['<none>']} "
                f'(did you mean "{sorted(commands[name])[0]}"?)'
            )
    assert not problems, "IPC contract broken:\n  " + "\n  ".join(problems)


def test_the_specific_bug_that_shipped_once_cannot_come_back():
    """run_outputs(run_id) is called as runId, not run_id."""
    commands = rust_commands()
    assert "runId" in commands["run_outputs"]
    assert "run_id" not in commands["run_outputs"]
    for name, keys in js_calls():
        if name == "run_outputs":
            assert "runId" in keys, f"run_outputs called with {keys}"
            assert "run_id" not in keys, "the camelCase rule was forgotten again"


def test_every_js_invoke_targets_a_real_command():
    commands = rust_commands()
    unknown = sorted({n for n, _ in js_calls() if n not in commands})
    assert not unknown, f"the UI invokes commands that do not exist: {unknown}"
