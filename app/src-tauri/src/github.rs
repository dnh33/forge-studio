//! GitHub API access for the forge-render pipeline.
//!
//! Auth is deliberately boring: the app reuses the GitHub CLI the developer
//! already has logged in. `gh auth token` prints the token; nothing is stored
//! by this app, and no credential ever reaches the webview.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const OWNER: &str = "dnh33";
pub const REPO: &str = "forge-render";
pub const WORKFLOW: &str = "render.yml";
pub const RENDERS_BRANCH: &str = "renders";
const API: &str = "https://api.github.com";
const RAW: &str = "https://raw.githubusercontent.com";

// ---------------------------------------------------------------- auth

/// Resolve a token: explicit env override first, then the gh CLI.
pub fn token() -> Result<String, String> {
    if let Ok(t) = std::env::var("FORGE_GH_TOKEN") {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    let out = std::process::Command::new("gh")
        .args(["auth", "token", "--hostname", "github.com"])
        .output()
        .map_err(|e| format!("GitHub CLI not found ({e}). Install `gh`, then run `gh auth login`."))?;
    if !out.status.success() {
        return Err("Not logged in to GitHub. Run `gh auth login` in a terminal, then reopen the studio.".into());
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
    h.insert(reqwest::header::USER_AGENT, "forge-studio/0.1".parse().unwrap());
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
        if !path.ends_with(".png") || path.ends_with(".cut.png") || path.ends_with(".depth.png")
            || path.ends_with(".glow.png") || path.contains("/sheets/")
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
