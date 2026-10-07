<p align="center">
  <img src="site/assets/wordmark.svg" alt="Forge Studio: render pipeline" width="840">
</p>

<p align="center">
  <b>A grimdark desktop studio for a free, public image pipeline.</b><br>
  Write a prompt set. Dispatch the run. Watch it stream. Take the images.
</p>

<p align="center">
  <a href="https://github.com/dnh33/forge-studio/releases"><img alt="download the latest release" src="https://img.shields.io/badge/download-latest%20release-c9a227?style=flat-square"></a>
  <a href="https://github.com/dnh33/forge-images"><img alt="the image pipeline" src="https://img.shields.io/badge/pipeline-forge--images-7a1f1f?style=flat-square"></a>
  <img alt="MIT licence" src="https://img.shields.io/badge/licence-MIT-4a3327?style=flat-square">
</p>

---

## What it is

Forge Studio is a desktop app that drives a free image pipeline. No GPU, no paid service, no credit card. It is three repositories with one idea: rendering should cost nothing and stay on your machine.

| repository | what it is |
|---|---|
| **[forge-images](https://github.com/dnh33/forge-images)** | the image pipeline. Prompt sets in, contact sheets out, rendered on GitHub's public runners. |
| **[forge-motion](https://github.com/dnh33/forge-motion)** | planned. Short video, the same shape. |
| **forge-studio** | this app. The desktop face of both. |

The forge is not idle. The studio is its console.

## What the studio does

- **Ideate.** Write a brief. A model through OpenRouter turns it into a coherent set: one shared style block, one concrete line per item, fixed seeds. It writes straight into the composer.
- **Compose.** Edit the set by hand: slug, canvas size, style block, items. Save it to the pipeline as `prompts/<slug>.json`, or fire it as a one-off that is never committed.
- **Dispatch.** Start the pipeline from the app. Choose seeds per item, steps, and shards.
- **Watch.** The run's status and every job stream in through the GitHub API. The tab carries a live ember while something is in flight.
- **Preview, then take it.** When the run finishes, the images it produced appear in a sheet with a download offer. Pick a folder and they land there.
- **Browse.** Everything ever published on the pipeline's `renders` branch, selectable, downloadable, openable in the browser.

## Why it is free, and what it costs

GitHub-hosted runners are free for public repositories, the image model is Apache-2.0, and the runtime is MIT. Nothing to buy, nothing to license.

The trade is time. Rendering is CPU only, so the pipeline shards a batch across parallel jobs. A measurement from this project: the model download takes about 2 minutes, and a single 768x1024 image at 4 steps took over 45 minutes on a 4-vCPU free runner. The batch is where the pipeline wins. A GPU is where it is fast.

## Your key stays yours

Ideation uses [OpenRouter](https://openrouter.ai). The key is handled one way and one way only.

1. If `OPENROUTER_API_KEY` is set in the environment, the Rust core reads it directly and **the UI never sees it**. This is the strongest path.
2. Otherwise, paste it into Settings once. It goes straight to the Rust core and into the OS credential store: Windows Credential Manager, macOS Keychain, or Secret Service.

It is never written to a config file, never logged, never placed in a URL, and never returned to the window. The field is cleared the moment it is saved.

Nothing else needs a secret. The pipeline is public, and the app reuses the GitHub CLI session you already have (`gh auth login`) instead of asking you for a token.

## Install

Download the installer from [Releases](https://github.com/dnh33/forge-studio/releases). Windows `.msi` or `.exe`, macOS `.dmg`.

Prerequisite for the GitHub side: a logged-in [GitHub CLI](https://cli.github.com) (`gh auth login`), which is where the app gets its token.

## Build

You do not need a toolchain. Every tag builds the installers on GitHub's own runners, attaches them to a **draft** release, and verifies it before it is made public. The landing page in `site/` is deployed by CI.

If you would rather build locally:

```bash
git clone https://github.com/dnh33/forge-studio
cd forge-studio/app
npm install
npm run tauri build
```

Requirements: Rust (stable) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

## Layout

```
app/
  ui/                the frontend: plain HTML/CSS/JS, no bundler
  src-tauri/src/
    github.rs        GitHub API: sets, dispatch, runs, renders, downloads
    openrouter.rs    ideation, and the only place the key is ever handled
    lib.rs           the command surface the UI talks to
site/                the landing page, deployed by CI
  assets/            wordmark.svg, favicon.svg, og.svg, og.png
```

The whole backend is a thin wrapper over two APIs. There is no local database and no cache: what the repository holds is what the app shows.

## Licence

MIT. The wordmark, favicon, and social card in `site/assets` are original artwork, set in Cinzel and IBM Plex Mono, both under the SIL Open Font License.
