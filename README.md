<p align="center">
  <img src="site/assets/wordmark.svg" alt="Forge Studio" width="880">
</p>

<p align="center">
  <b>A grimdark desktop studio for a free, public image pipeline.</b><br>
  Write prompt sets. Dispatch the run. Watch it live. Take the download.
</p>

<p align="center">
  <a href="https://github.com/dnh33/forge-studio/releases"><img alt="Download" src="https://img.shields.io/badge/download-latest%20release-c9a227?style=flat-square"></a>
  <a href="https://github.com/dnh33/forge-render"><img alt="Pipeline" src="https://img.shields.io/badge/pipeline-forge--render-7a1f1f?style=flat-square"></a>
  <img alt="Licence" src="https://img.shields.io/badge/licence-MIT-4a3327?style=flat-square">
</p>

---

## What this is

Three repositories, one idea: **rendering should cost nothing and live on your machine.**

| | |
|---|---|
| **[forge-render](https://github.com/dnh33/forge-render)** | the image pipeline — prompt sets in, contact sheets out, rendered on GitHub's free public runners |
| **[forge-motion](https://github.com/dnh33/forge-motion)** | the motion pipeline — the same shape, for short video |
| **forge-studio** | this app — the desktop face of both |

You do not need a GPU, an API key or a credit card to make images. You need a repository and some patience.

## What the studio does

- **Ideate.** Write a brief; a model through OpenRouter turns it into a coherent set: a shared style
  block, one concrete line per item, fixed seeds. It writes straight into the composer.
- **Compose.** Edit the set by hand — slug, canvas, style block, items. Save it to the repository as
  `prompts/<slug>.json`, or fire it as a one-off that is never committed.
- **Dispatch.** Start the pipeline from the app. Choose seeds per item, steps and shards.
- **Watch.** The run's status and every job stream in through the GitHub API. The tab shows the live
  ember while something is in flight.
- **Preview, then take it.** When the run finishes, the images it produced appear in a sheet with a
  download offer. Choose a folder, and they land there.
- **Gallery.** Everything ever published on the `renders` branch, selectable, downloadable, openable
  in the browser.

## Your key stays yours

Ideation uses [OpenRouter](https://openrouter.ai). The key is handled like this, and only like this:

1. If `OPENROUTER_API_KEY` is set in the environment, it is read directly by the Rust core —
   **the UI never sees it at all**. This is the strongest path.
2. Otherwise, paste it into Settings once. It goes straight to the Rust core and into the OS
   credential store (Windows Credential Manager / macOS Keychain / Secret Service).
3. It is never written to a config file, never logged, never put in a URL, and never returned to the
   window. The field is cleared the moment it is saved.

Nothing else in the app needs a secret: the pipeline is public, and the app reuses the GitHub CLI
session you already have (`gh auth login`) rather than asking you for a token.

## Install

Download the installer from [Releases](https://github.com/dnh33/forge-studio/releases).
Windows `.msi`/`.exe`, macOS `.dmg`.

Prerequisite for the GitHub side: a logged-in [GitHub CLI](https://cli.github.com) (`gh auth login`),
which is where the app gets its token.

## Build

The point of this repository is that **you do not need a toolchain**. Every tag builds the installers
on GitHub's own runners and attaches them to a **draft** release, which is verified before it is made
public. Cloudflare Pages (or GitHub Pages — it is wired up in `.github/workflows/pages.yml`) serves
`site/`.

If you do want to build locally:

```bash
git clone https://github.com/dnh33/forge-studio
cd forge-studio/app
npm install
npm run tauri build
```

Requirements: Rust (stable) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for
your platform.

## Layout

```
app/
  ui/                 the frontend — plain HTML/CSS/JS, no bundler
  src-tauri/src/
    github.rs         GitHub API: sets, dispatch, runs, renders, downloads
    openrouter.rs     ideation; the only place the key is ever handled
    lib.rs            the command surface the UI talks to
site/                 the landing page (deployed by CI)
```

The whole backend is a thin wrapper over two APIs. There is no local database and no cache: what the
repository holds is what the app shows.

## Licence

MIT.
