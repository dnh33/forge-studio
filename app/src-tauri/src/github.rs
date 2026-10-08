//! GitHub API access for the forge-images pipeline.
//!
//! Auth is deliberately boring: the app reuses the GitHub CLI the developer
//! already has logged in. `gh auth token` prints the token; nothing is stored
//! by this app, and no credential ever reaches the webview.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const OWNER: &str = "dnh33";
pub const REPO: &str = "forge-images";
pub const WORKFLOW: &str = "render.yml";
pub const RENDERS_BRANCH: &str = "renders";
const API: &str = "https://api.github.com";
const RAW: &str = "https://raw.githubusercontent.com";

// ---------------------------------------------------------------- auth

/// The token is resolved once and reused. Resolving it spawns the `gh` CLI, and
/// on Windows that would flash a console window on every poll without the
/// CREATE_NO_WINDOW flag below.
static TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Resolve a token: explicit env override first, then the gh CLI. Cached after
/// the first successful resolve.
pub fn token() -> Result<String, String> {
    if let Some(t) = TOKEN.get() {
        return Ok(t.clone());
    }
    let t = resolve_token()?;
    let _ = TOKEN.set(t.clone());
    Ok(t)
}

fn resolve_token() -> Result<String, String> {
    if let Ok(t) = std::env::var("FORGE_GH_TOKEN") {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["auth", "token", "--hostname", "github.com"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: a GUI app must not flash a console for every child process.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| {
        format!("GitHub CLI not found ({e}). Install `gh`, then run `gh auth login`.")
    })?;
    if !out.status.success() {
        return Err(
            "Not logged in to GitHub. Run `gh auth login` in a terminal, then reopen the studio."
                .into(),
        );
    }
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if t.is_empty() {
        return Err("`gh auth token` returned nothing. Run `gh auth login`.".into());
    }
    Ok(t)
}

pub fn client() -> Result<reqwest::Client, String> {
    let t = token()?;
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {t}").parse().map_err(|_| "bad token")?,
    );
    h.insert(
        reqwest::header::USER_AGENT,
        "forge-studio/0.1".parse().unwrap(),
    );
    h.insert(
        reqwest::header::ACCEPT,
        "application/vnd.github+json".parse().unwrap(),
    );
    h.insert("X-GitHub-Api-Version", "2022-11-28".parse().unwrap());
    reqwest::Client::builder()
        .default_headers(h)
        .build()
        .map_err(|e| e.to_string())
}

async fn get(c: &reqwest::Client, url: &str) -> Result<Value, String> {
    let r = c.get(url).send().await.map_err(|e| e.to_string())?;
    let s = r.status();
    let body = r.text().await.map_err(|e| e.to_string())?;
    if !s.is_success() {
        return Err(format!("GitHub {s}: {}", short(&body)));
    }
    serde_json::from_str(&body).map_err(|e| format!("bad JSON from {url}: {e}"))
}

fn short(s: &str) -> String {
    let t: String = s.chars().take(300).collect();
    t
}

// ---------------------------------------------------------------- types

#[derive(Serialize, Deserialize, Clone)]
pub struct PromptItem {
    pub seed: i64,
    pub line: String,
}

fn default_size() -> [u32; 2] {
    [768, 1024]
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PromptSet {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_size")]
    pub size: [u32; 2],
    #[serde(default)]
    pub style: String,
    pub items: std::collections::BTreeMap<String, PromptItem>,
}

#[derive(Serialize)]
pub struct SetSummary {
    pub slug: String,
    pub name: String,
    pub size: [u32; 2],
    pub items: usize,
    pub sha: String,
    pub edited: bool,
}

#[derive(Serialize)]
pub struct Identity {
    pub login: String,
    pub name: String,
    pub avatar_url: String,
    pub source: String,
}

#[derive(Serialize)]
pub struct RunSummary {
    pub id: u64,
    pub number: u64,
    pub status: String,
    pub conclusion: Option<String>,
    pub title: String,
    pub created_at: String,
    pub html_url: String,
}

#[derive(Serialize)]
pub struct JobSummary {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Serialize)]
pub struct RunDetail {
    pub run: RunSummary,
    pub jobs: Vec<JobSummary>,
}

#[derive(Serialize)]
pub struct RenderImage {
    pub set: String,
    pub file: String,
    pub url: String,
    pub meta_url: String,
}

// ---------------------------------------------------------------- identity

pub async fn identity() -> Result<Identity, String> {
    let c = client()?;
    let v = get(&c, &format!("{API}/user")).await?;
    let source = if std::env::var("FORGE_GH_TOKEN").is_ok() {
        "env:FORGE_GH_TOKEN"
    } else {
        "gh CLI"
    };
    Ok(Identity {
        login: v["login"].as_str().unwrap_or("").into(),
        name: v["name"].as_str().unwrap_or("").into(),
        avatar_url: v["avatar_url"].as_str().unwrap_or("").into(),
        source: source.into(),
    })
}

// ---------------------------------------------------------------- prompt sets

pub async fn list_sets() -> Result<Vec<SetSummary>, String> {
    let c = client()?;
    let v = get(
        &c,
        &format!("{API}/repos/{OWNER}/{REPO}/contents/prompts?ref=main"),
    )
    .await?;
    let mut out = Vec::new();
    for e in v.as_array().cloned().unwrap_or_default() {
        let name = e["name"].as_str().unwrap_or("");
        if !name.ends_with(".json") {
            continue;
        }
        let slug = name.trim_end_matches(".json").to_string();
        let durl = e["download_url"].as_str().unwrap_or("");
        let set = fetch_set_raw(&c, durl).await.ok();
        let (title, size, items) = match set {
            Some(s) => (
                s.name.unwrap_or_else(|| slug.clone()),
                s.size,
                s.items.len(),
            ),
            None => (slug.clone(), default_size(), 0),
        };
        out.push(SetSummary {
            slug,
            name: title,
            size,
            items,
            sha: e["sha"].as_str().unwrap_or("").into(),
            edited: true,
        });
    }
    out.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(out)
}

async fn fetch_set_raw(c: &reqwest::Client, url: &str) -> Result<PromptSet, String> {
    let r = c.get(url).send().await.map_err(|e| e.to_string())?;
    let body = r.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

pub async fn get_set(slug: String) -> Result<PromptSet, String> {
    let c = client()?;
    let url = format!("{RAW}/{OWNER}/{REPO}/main/prompts/{slug}.json");
    fetch_set_raw(&c, &url).await
}

/// Create or update `prompts/<slug>.json` on main. `body` is the raw JSON text.
pub async fn save_set(slug: String, body: String) -> Result<String, String> {
    let c = client()?;
    // Validate before writing, so a malformed set never lands on main.
    serde_json::from_str::<PromptSet>(&body).map_err(|e| format!("Not a valid prompt set: {e}"))?;
    let path = format!("prompts/{slug}.json");
    let api = format!("{API}/repos/{OWNER}/{REPO}/contents/{path}");
    let existing = c.get(&api).send().await.map_err(|e| e.to_string())?;
    let sha = if existing.status().is_success() {
        let v: Value = existing.json().await.unwrap_or(json!({}));
        v["sha"].as_str().map(|s| s.to_string())
    } else {
        None
    };
    let mut payload = json!({
        "message": format!("forge-studio: save prompt set '{slug}'"),
        "content": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, body.as_bytes()),
        "branch": "main",
    });
    if let Some(s) = sha {
        payload["sha"] = json!(s);
    }
    let r = c
        .put(&api)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let st = r.status();
    let txt = r.text().await.unwrap_or_default();
    if !st.is_success() {
        return Err(format!("GitHub {st}: {}", short(&txt)));
    }
    let v: Value = serde_json::from_str(&txt).unwrap_or(json!({}));
    Ok(v["commit"]["sha"].as_str().unwrap_or("").into())
}

// ---------------------------------------------------------------- decisions

/// Verdicts and reasons are CLOSED sets. A free-text reason would rot into
/// unusable prose within a month, and this file is the compounding record.
const VERDICTS: [&str; 3] = ["keep", "reject", "undecided"];
const REASONS: [&str; 7] = [
    "muddy",
    "off-style",
    "wrong-subject",
    "wrong-composition",
    "artifacts",
    "duplicate",
    "close-but-off",
];
const NOTE_MAX: usize = 200;
const DECISION_FETCH_CAP: usize = 200;

pub struct Decision {
    pub set: String,
    pub file: String,
    pub verdict: String,
    pub reason: Option<String>,
    pub note: Option<String>,
}

fn plain_name(value: &str, what: &str) -> Result<(), String> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || value.contains('\0')
    {
        return Err(format!("{what} must be a plain name, got {value:?}"));
    }
    Ok(())
}

/// Everything that can be rejected about a decision, with no token, no network,
/// and no clock. Kept separate so the rules are testable in isolation: a test
/// that called `save_decision` itself could reach `gh auth token` on a runner
/// where gh is installed and write to the repository by accident.
pub fn validate_decision(d: &Decision) -> Result<(), String> {
    plain_name(&d.set, "set")?;
    plain_name(&d.file, "file")?;
    if !VERDICTS.contains(&d.verdict.as_str()) {
        return Err(format!(
            "verdict must be one of {VERDICTS:?}, got {:?}",
            d.verdict
        ));
    }
    if let Some(r) = d.reason.as_deref() {
        if !REASONS.contains(&r) {
            return Err(format!("reason must be one of {REASONS:?}, got {r:?}"));
        }
    }
    let note = d.note.clone().unwrap_or_default();
    let note = note.trim();
    if note.chars().count() > NOTE_MAX {
        return Err(format!("note is limited to {NOTE_MAX} characters"));
    }
    Ok(())
}

/// Write `renders/<set>/<image>.decision.json` on the renders branch, beside the
/// image and its sidecar, so the verdict is git-versioned with the render.
pub async fn save_decision(d: Decision) -> Result<String, String> {
    validate_decision(&d)?;

    let note = d.note.unwrap_or_default();
    let note = note.trim();

    let stem = d.file.trim_end_matches(".png").trim_end_matches(".jpg");
    let path = format!("renders/{}/{stem}.decision.json", d.set);
    let at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = json!({
        "image": d.file,
        "verdict": d.verdict,
        "reason": d.reason,
        "note": if note.is_empty() { Value::Null } else { json!(note) },
        "at_unix": at_unix,
    })
    .to_string();

    let c = client()?;
    let api = format!("{API}/repos/{OWNER}/{REPO}/contents/{path}");
    let existing = c.get(&api).send().await.map_err(|e| e.to_string())?;
    let sha = if existing.status().is_success() {
        let v: Value = existing.json().await.unwrap_or(json!({}));
        v["sha"].as_str().map(|s| s.to_string())
    } else {
        None
    };
    let mut payload = json!({
        "message": format!("forge-studio: {} {}", d.verdict, stem),
        "content": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, body.as_bytes()),
        "branch": RENDERS_BRANCH,
    });
    if let Some(s) = sha {
        payload["sha"] = json!(s);
    }
    let r = c
        .put(&api)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let st = r.status();
    let txt = r.text().await.unwrap_or_default();
    if !st.is_success() {
        return Err(format!("GitHub {st}: {}", short(&txt)));
    }
    let v: Value = serde_json::from_str(&txt).unwrap_or(json!({}));
    Ok(v["commit"]["sha"].as_str().unwrap_or("").into())
}

/// Every recorded decision for one set, keyed by its path. Capped, and tolerant
/// of a single unreadable file: a decision that cannot be parsed is skipped
/// rather than failing the whole pass.
pub async fn decisions(set: String) -> Result<Value, String> {
    plain_name(&set, "set")?;
    let c = client()?;
    let url = format!("{API}/repos/{OWNER}/{REPO}/git/trees/{RENDERS_BRANCH}?recursive=1");
    let v = get(&c, &url).await?;
    let prefix = format!("renders/{set}/");
    let mut out = serde_json::Map::new();
    for entry in v["tree"].as_array().cloned().unwrap_or_default() {
        if out.len() >= DECISION_FETCH_CAP {
            break;
        }
        let path = entry["path"].as_str().unwrap_or("").to_string();
        if !path.starts_with(&prefix) || !path.ends_with(".decision.json") {
            continue;
        }
        let api = format!("{API}/repos/{OWNER}/{REPO}/contents/{path}?ref={RENDERS_BRANCH}");
        let Ok(file) = get(&c, &api).await else {
            continue;
        };
        let raw: String = file["content"]
            .as_str()
            .unwrap_or("")
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        let Ok(bytes) =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, raw.as_bytes())
        else {
            continue;
        };
        if let Ok(d) = serde_json::from_slice::<Value>(&bytes) {
            out.insert(path, d);
        }
    }
    Ok(Value::Object(out))
}

// ---------------------------------------------------------------- previews

#[derive(Serialize)]
pub struct PreviewImage {
    pub file: String,
    pub data_url: String,
}

/// The preview images a run produced, as data URLs.
///
/// A preview is never published to the renders branch, so it exists only as a run
/// artifact. GitHub serves an artifact zip from a storage host *after a
/// redirect*, and a bearer token must not follow it, so the redirect is refused
/// by policy and resolved by hand, then the signed URL is fetched with no
/// Authorization header at all.
pub async fn run_previews(run_id: u64) -> Result<Vec<PreviewImage>, String> {
    let api = client()?;
    let list = get(
        &api,
        &format!("{API}/repos/{OWNER}/{REPO}/actions/runs/{run_id}/artifacts"),
    )
    .await?;

    // Same credentials, but never follow a redirect with them attached.
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {}", token()?)
            .parse()
            .map_err(|_| "bad token")?,
    );
    headers.insert(
        reqwest::header::USER_AGENT,
        "forge-studio/0.1".parse().unwrap(),
    );
    let strict = reqwest::Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let plain = reqwest::Client::builder()
        .user_agent("forge-studio/0.1")
        .build()
        .map_err(|e| e.to_string())?;

    const CAP: usize = 24;
    let mut out = Vec::new();
    for artifact in list["artifacts"].as_array().cloned().unwrap_or_default() {
        if out.len() >= CAP {
            break;
        }
        let name = artifact["name"].as_str().unwrap_or("");
        if !name.starts_with("preview-") {
            continue;
        }
        let id = artifact["id"].as_u64().unwrap_or(0);
        let resp = strict
            .get(format!(
                "{API}/repos/{OWNER}/{REPO}/actions/artifacts/{id}/zip"
            ))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let bytes = if resp.status().is_redirection() {
            let signed = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("artifact redirect carried no Location")?
                .to_string();
            let r = plain.get(&signed).send().await.map_err(|e| e.to_string())?;
            if !r.status().is_success() {
                continue;
            }
            r.bytes().await.map_err(|e| e.to_string())?
        } else if resp.status().is_success() {
            resp.bytes().await.map_err(|e| e.to_string())?
        } else {
            continue;
        };

        let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
            continue;
        };
        for i in 0..archive.len() {
            if out.len() >= CAP {
                break;
            }
            let Ok(mut entry) = archive.by_index(i) else {
                continue;
            };
            let entry_name = entry.name().to_string();
            if !entry_name.ends_with(".png") {
                continue;
            }
            let mut buf = Vec::new();
            if std::io::Read::read_to_end(&mut entry, &mut buf).is_err() {
                continue;
            }
            out.push(PreviewImage {
                file: entry_name
                    .rsplit('/')
                    .next()
                    .unwrap_or(&entry_name)
                    .to_string(),
                data_url: format!(
                    "data:image/png;base64,{}",
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buf)
                ),
            });
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(out)
}

// ---------------------------------------------------------------- dispatch

pub struct DispatchOpts {
    pub set: String,
    pub only: String,
    pub variants: String,
    pub steps: String,
    pub shards: String,
    pub adhoc: String,
}

pub async fn dispatch(o: DispatchOpts) -> Result<(), String> {
    let c = client()?;
    let url = format!("{API}/repos/{OWNER}/{REPO}/actions/workflows/{WORKFLOW}/dispatches");
    let r = c
        .post(&url)
        .json(&json!({
            "ref": "main",
            "inputs": {
                "set": o.set,
                "only": o.only,
                "variants": o.variants,
                "steps": o.steps,
                "shards": o.shards,
                "adhoc": o.adhoc,
            }
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let st = r.status();
    if !st.is_success() {
        let t = r.text().await.unwrap_or_default();
        return Err(format!("GitHub {st}: {}", short(&t)));
    }
    Ok(())
}

// ---------------------------------------------------------------- runs

fn run_summary(v: &Value) -> RunSummary {
    RunSummary {
        id: v["id"].as_u64().unwrap_or(0),
        number: v["run_number"].as_u64().unwrap_or(0),
        status: v["status"].as_str().unwrap_or("").into(),
        conclusion: v["conclusion"].as_str().map(|s| s.to_string()),
        title: v["display_title"].as_str().unwrap_or("").into(),
        created_at: v["created_at"].as_str().unwrap_or("").into(),
        html_url: v["html_url"].as_str().unwrap_or("").into(),
    }
}

pub async fn list_runs(limit: u32) -> Result<Vec<RunSummary>, String> {
    let c = client()?;
    let v = get(
        &c,
        &format!("{API}/repos/{OWNER}/{REPO}/actions/runs?per_page={limit}"),
    )
    .await?;
    Ok(v["workflow_runs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(run_summary)
        .collect())
}

pub async fn get_run(id: u64) -> Result<RunDetail, String> {
    let c = client()?;
    let v = get(&c, &format!("{API}/repos/{OWNER}/{REPO}/actions/runs/{id}")).await?;
    let jobs_v = get(
        &c,
        &format!("{API}/repos/{OWNER}/{REPO}/actions/runs/{id}/jobs"),
    )
    .await?;
    let jobs = jobs_v["jobs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|j| JobSummary {
            name: j["name"].as_str().unwrap_or("").into(),
            status: j["status"].as_str().unwrap_or("").into(),
            conclusion: j["conclusion"].as_str().map(|s| s.to_string()),
            html_url: j["html_url"].as_str().unwrap_or("").into(),
            started_at: j["started_at"].as_str().map(|s| s.to_string()),
            completed_at: j["completed_at"].as_str().map(|s| s.to_string()),
        })
        .collect();
    Ok(RunDetail {
        run: run_summary(&v),
        jobs,
    })
}

// ---------------------------------------------------------------- renders

pub async fn list_renders() -> Result<Vec<RenderImage>, String> {
    let c = client()?;
    let url = format!("{API}/repos/{OWNER}/{REPO}/git/trees/{RENDERS_BRANCH}?recursive=1");
    let r = c.get(&url).send().await.map_err(|e| e.to_string())?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(vec![]); // no run has published yet
    }
    let v: Value = r.json().await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for e in v["tree"].as_array().cloned().unwrap_or_default() {
        let path = e["path"].as_str().unwrap_or("");
        if !path.ends_with(".png")
            || path.ends_with(".cut.png")
            || path.ends_with(".depth.png")
            || path.ends_with(".glow.png")
            || path.contains("/sheets/")
        {
            continue;
        }
        let file = path.rsplit('/').next().unwrap_or(path).to_string();
        let set = path.split('/').nth(1).unwrap_or("renders").to_string();
        out.push(RenderImage {
            set,
            file: file.clone(),
            url: format!("{RAW}/{OWNER}/{REPO}/{RENDERS_BRANCH}/{path}"),
            meta_url: format!(
                "{RAW}/{OWNER}/{REPO}/{RENDERS_BRANCH}/{}",
                path.trim_end_matches(".png").to_string() + ".json"
            ),
        });
    }
    out.sort_by(|a, b| a.set.cmp(&b.set).then(a.file.cmp(&b.file)));
    Ok(out)
}

/// Exactly what a given run added to the renders branch: walk the publish
/// commit for that run id and diff it against its parent.
pub async fn run_outputs(run_id: u64) -> Result<Vec<RenderImage>, String> {
    let c = client()?;
    let commits = get(
        &c,
        &format!("{API}/repos/{OWNER}/{REPO}/commits?sha={RENDERS_BRANCH}&per_page=30"),
    )
    .await?;
    let list = commits.as_array().cloned().unwrap_or_default();
    let marker = format!("Renders from run {run_id}");
    let mut found: Option<(String, Option<String>)> = None;
    for cm in &list {
        let msg = cm["commit"]["message"].as_str().unwrap_or("");
        if msg.contains(&marker) {
            let sha = cm["sha"].as_str().unwrap_or("").to_string();
            let parent = cm["parents"]
                .as_array()
                .and_then(|p| p.first())
                .and_then(|p| p["sha"].as_str())
                .map(|s| s.to_string());
            found = Some((sha, parent));
            break;
        }
    }
    let (sha, parent) = match found {
        Some(v) => v,
        None => return Ok(vec![]), // still publishing, or nothing committed
    };
    let mut paths: Vec<String> = Vec::new();
    if let Some(p) = parent {
        let cmp = get(
            &c,
            &format!("{API}/repos/{OWNER}/{REPO}/compare/{p}...{sha}"),
        )
        .await?;
        for f in cmp["files"].as_array().cloned().unwrap_or_default() {
            let name = f["filename"].as_str().unwrap_or("");
            if f["status"].as_str() == Some("added") {
                paths.push(name.to_string());
            }
        }
    }
    let mut out = Vec::new();
    for path in paths {
        if !path.ends_with(".png") || path.contains("/sheets/") {
            continue;
        }
        let file = path.rsplit('/').next().unwrap_or(&path).to_string();
        let set = path.split('/').nth(1).unwrap_or("renders").to_string();
        out.push(RenderImage {
            set,
            file: file.clone(),
            url: format!("{RAW}/{OWNER}/{REPO}/{sha}/{path}"),
            meta_url: format!(
                "{RAW}/{OWNER}/{REPO}/{sha}/{}",
                path.trim_end_matches(".png").to_string() + ".json"
            ),
        });
    }
    out.sort_by(|a, b| a.set.cmp(&b.set).then(a.file.cmp(&b.file)));
    Ok(out)
}

/// Download one image (raw URL, no auth needed for a public repo) into `dir`.
pub async fn download(url: String, dir: String) -> Result<String, String> {
    let c = reqwest::Client::builder()
        .user_agent("forge-studio/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    let r = c.get(&url).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("Download failed: HTTP {}", r.status()));
    }
    let bytes = r.bytes().await.map_err(|e| e.to_string())?;
    let file = url.rsplit('/').next().unwrap_or("image.png").to_string();
    let dest = std::path::Path::new(&dir).join(file);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

#[cfg(test)]
mod decision_tests {
    use super::*;

    fn base() -> Decision {
        Decision {
            set: "portraits".into(),
            file: "portraits-marshal-s1101.png".into(),
            verdict: "keep".into(),
            reason: None,
            note: None,
        }
    }

    // Validation runs BEFORE any token or network call, so these are offline.

    #[test]
    fn an_unknown_verdict_is_refused() {
        let mut d = base();
        d.verdict = "brilliant".into();
        let e = validate_decision(&d).unwrap_err();
        assert!(e.contains("verdict"), "{e}");
    }

    #[test]
    fn an_unknown_reason_is_refused() {
        let mut d = base();
        d.verdict = "reject".into();
        d.reason = Some("meh".into());
        let e = validate_decision(&d).unwrap_err();
        assert!(e.contains("reason"), "{e}");
    }

    #[test]
    fn every_documented_reason_is_accepted() {
        for r in REASONS {
            let mut d = base();
            d.verdict = "reject".into();
            d.reason = Some(r.to_string());
            validate_decision(&d).unwrap_or_else(|e| panic!("reason {r} was rejected: {e}"));
        }
    }

    #[test]
    fn an_undecided_or_kept_verdict_needs_no_reason() {
        for v in VERDICTS {
            let mut d = base();
            d.verdict = v.into();
            validate_decision(&d).unwrap_or_else(|e| panic!("verdict {v} was rejected: {e}"));
        }
    }

    #[test]
    fn an_overlong_note_is_refused() {
        let mut d = base();
        d.note = Some("x".repeat(NOTE_MAX + 1));
        let e = validate_decision(&d).unwrap_err();
        assert!(e.contains("note"), "{e}");
    }

    #[test]
    fn a_note_at_the_limit_is_accepted() {
        let mut d = base();
        d.note = Some("x".repeat(NOTE_MAX));
        validate_decision(&d).expect("a note at the limit is allowed");
    }

    #[test]
    fn a_note_is_measured_in_characters_not_bytes() {
        let mut d = base();
        // 200 multi-byte characters are 600 bytes but still within the limit.
        d.note = Some("ø".repeat(NOTE_MAX));
        validate_decision(&d).expect("a 200-character note is allowed whatever its byte length");
    }

    #[test]
    fn a_path_in_the_set_or_the_file_is_refused() {
        for bad in ["../etc", "a/b", "a\\b", ""] {
            let mut d = base();
            d.set = bad.into();
            assert!(validate_decision(&d).unwrap_err().contains("plain name"));

            let mut d = base();
            d.file = bad.into();
            assert!(validate_decision(&d).unwrap_err().contains("plain name"));
        }
    }
}
