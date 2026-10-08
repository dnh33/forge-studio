# ADR-0003 — One-click onboarding, and what can actually be locked

Status: **APPROVED** 2026-10-08 (direction). Design below; confirm the threat model
before building the gates.

## Context

Two requirements arrived together:

1. Other people should be able to hook the studio up to **their own** GitHub and
   Cloudflare accounts with something close to one click.
2. Only the pipelines that are Danie's, running under `dnh33`, should be usable —
   nothing else should be able to drive them.

These pull in opposite directions and are only compatible if "the lock" is precise
about what it protects. So state first what is already true:

- **A stranger cannot run his pipelines.** `workflow_dispatch` requires write access
  to the repository, and pushing to `renders` requires it too. Anyone can *fork* and
  run their own copy on their own minutes; nobody can spend his.
- **A stranger cannot read the app's control plane.** It is loopback-only and
  bearer-token gated, and the token lives in the user's profile.
- **A stranger can read his prompts and images**, because the repository is public.
  That is exposure, not access, and it is the finding Danie has not yet decided on.

## Decision

**Onboarding** uses GitHub's create-from-template, not forks:

- The pipeline ships as a **template repository**. Onboarding calls
  `POST /repos/{owner}/{template}/generate`, which creates a full, independent copy
  in the user's own account in one call, with no fork relationship and no network of
  forks to maintain.
- Their Actions run on their account and their quota. The generated repository holds
  `context/facts.json` with their login, so the workflows can be configured for them.
- **Cloudflare is opt-in and separate**: a Cloudflare Pages project for the site, and
  optionally a Worker. Its API token is stored in the OS keyring like the OpenRouter
  key already is, never in the webview and never in the repository.
- The studio is **scoped to exactly one repository**, chosen at onboarding. There is
  no cross-account path: the app talks to one repo with the user's own token.

**The lock** is therefore several small, honest gates rather than one wall:

| layer | what it stops |
|---|---|
| GitHub's own permissions | strangers dispatching in his repo or pushing to `renders` |
| **Actor gate** in the render workflow: refuse unless `github.actor` is the owner recorded at setup | a collaborator, a bot, or an accidentally-wrong account spending compute |
| Control plane: loopback + bearer token | any remote caller |
| App scoped to one configured repository | the app wandering onto another repo |

The actor gate is one condition and the template writes the owner's login into it,
so it is correct for his account and for everyone else's without a fork.

## What cannot be locked, stated plainly

- **A public repository's prompts and renders are readable by anyone.** No gate
  changes that. If the prompts matter, the repository must be private, and that
  trades away unlimited free Action minutes for roughly 40 images a month.
- **Anyone may fork and self-host.** That is the point of a template, and it costs
  him nothing.
- **A run's inputs and logs are visible for a public repo**, so moving prompts out
  of the repository and into dispatch inputs would hide them from `prompts/` while
  leaving them in the run UI. Not a fix; do not pretend otherwise.

## Open questions for Danie

1. Public and free, or private and metered? This is the only decision here that
   costs money.
2. Is Cloudflare wanted for the **site** (Pages) or for a **Worker**? They need
   different onboarding.
3. Should the actor gate be a hard refusal, or a loud warning in the run summary?
