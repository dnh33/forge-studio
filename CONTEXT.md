# CONTEXT.md — glossary

Terms as this repository means them. Keep it to vocabulary; anything that is a
rule belongs in `AGENTS.md` or an ADR.

| term | meaning |
|---|---|
| **forge** | The whole ecosystem: the pipeline, the studio, the motion repo. |
| **set** | A named group of prompt lines, e.g. `portraits`. Lives as `prompts/<slug>.json` in `forge-images`. |
| **item** | One line in a set: an id, a prompt line, and a seed. One item renders to one image. |
| **shard** | One matrix job of a render run. `MAX_PER_SHARD` caps items per shard to stay inside runner limits. |
| **render** | A published image on the `renders` branch, with a JSON sidecar recording prompt, seed and settings. |
| **preview** | A deliberately cheap render: quarter-area canvas, 2 steps, ~220 s instead of ~2699 s. **Never published** — it exists only as a run artifact, which is why the app reads artifacts to show one. |
| **contact sheet** | The grid image a run produces so a whole set can be judged at a glance. |
| **ledger** | The triage record: `<image>.decision.json` beside each render, holding verdict, reason and note. Git-versioned with the image it judges. |
| **verdict** | `keep`, `reject` or `undecided`. Closed set. |
| **reason** | The closed seven: `muddy`, `off-style`, `wrong-subject`, `wrong-composition`, `artifacts`, `duplicate`, `close-but-off`. |
| **run** | One execution of the render workflow. Identified by a GitHub run id. |
| **publish** | The job that commits renders to the `renders` branch. Skipped for previews, by design. |
| **control plane** | The loopback, token-gated HTTP API the running app exposes on 127.0.0.1:7317. |
| **descriptor** | `%APPDATA%\forge-studio\control.json` — port, token, pid. How a client discovers the control plane. |
| **engine** | In this repo, the not-yet-existing headless half of the app (ADR-0001). Today the control plane lives inside the GUI process. |
| **viewport** | The window, once the engine is split out (ADR-0001). |
| **schnell** | FLUX.1-schnell Q4_K_S, the default image model. |
| **motion** | LTX-Video image to video, in `forge-motion`. Local GPU, EXPERIMENTAL. |
| **dnh33** | Danie's GitHub account, and the owner of the render account in the lock design (ADR-0003). |
