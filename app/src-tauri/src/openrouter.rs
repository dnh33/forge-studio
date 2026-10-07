//! OpenRouter access for ideation.
//!
//! Security posture, stated plainly:
//!   * The API key is read from the environment (`OPENROUTER_API_KEY`) if set —
//!     that path never touches the webview at all.
//!   * Otherwise it is stored in the OS credential store (Windows Credential
//!     Manager / macOS Keychain / Secret Service) through the `keyring` crate.
//!   * The key is NEVER returned to the frontend, never written to a config
//!     file, never logged and never put in a URL.
//!   * Everything that uses it happens in this process.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const BASE: &str = "https://openrouter.ai/api/v1";
const SERVICE: &str = "forge-studio";
const USER_KEY: &str = "openrouter-api-key";
const USER_MODEL: &str = "openrouter-model";

// ---------------------------------------------------------------- key storage

fn entry(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, user).map_err(|e| format!("credential store unavailable: {e}"))
}

pub fn key() -> Option<String> {
    if let Ok(k) = std::env::var("OPENROUTER_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_string());
        }
    }
    entry(USER_KEY).ok()?.get_password().ok().filter(|s| !s.is_empty())
}

fn key_source() -> &'static str {
    if std::env::var("OPENROUTER_API_KEY").map(|v| !v.trim().is_empty()).unwrap_or(false) {
        "environment"
    } else if key().is_some() {
        "OS credential store"
    } else {
        "none"
    }
}

pub fn set_key(k: String) -> Result<(), String> {
    let k = k.trim().to_string();
    if k.is_empty() {
        return Err("empty key".into());
    }
    entry(USER_KEY)?.set_password(&k).map_err(|e| e.to_string())
}

pub fn clear_key() -> Result<(), String> {
    if let Ok(e) = entry(USER_KEY) {
        let _ = e.delete_credential(); // best effort; absent is fine
    }
    Ok(())
}

pub fn get_model() -> Option<String> {
    entry(USER_MODEL).ok()?.get_password().ok().filter(|s| !s.is_empty())
}

pub fn set_model(m: String) -> Result<(), String> {
    entry(USER_MODEL)?.set_password(m.trim()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- types

#[derive(Serialize)]
pub struct OrStatus {
    pub configured: bool,
    pub source: String,
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OrModel {
    pub id: String,
    pub name: String,
    pub context: u64,
    pub prompt_price: f64,
    pub completion_price: f64,
}

#[derive(Serialize)]
pub struct OrKeyCheck {
    pub label: String,
    pub usage: f64,
    pub limit: Option<f64>,
    pub is_free_tier: bool,
}

// ---------------------------------------------------------------- API

pub fn status() -> OrStatus {
    OrStatus {
        configured: key().is_some(),
        source: key_source().to_string(),
        model: get_model(),
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("forge-studio/0.1")
        .build()
        .map_err(|e| e.to_string())
}

pub async fn models() -> Result<Vec<OrModel>, String> {
    let c = client()?;
    let r = c
        .get(format!("{BASE}/models"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let v: Value = r.json().await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for m in v["data"].as_array().cloned().unwrap_or_default() {
        let id = m["id"].as_str().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        let p = &m["pricing"];
        out.push(OrModel {
            name: m["name"].as_str().unwrap_or(&id).to_string(),
            id,
            context: m["context_length"].as_u64().unwrap_or(0),
            prompt_price: p["prompt"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            completion_price: p["completion"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

pub async fn verify_key() -> Result<OrKeyCheck, String> {
    let k = key().ok_or("No OpenRouter key configured.")?;
    let c = client()?;
    let r = c
        .get(format!("{BASE}/key"))
        .header("Authorization", format!("Bearer {k}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let st = r.status();
    let v: Value = r.json().await.unwrap_or(json!({}));
    if !st.is_success() {
        return Err(format!("OpenRouter rejected the key: HTTP {st}"));
    }
    let d = &v["data"];
    Ok(OrKeyCheck {
        label: d["label"].as_str().unwrap_or("key").to_string(),
        usage: d["usage"].as_f64().unwrap_or(0.0),
        limit: d["limit"].as_f64(),
        is_free_tier: d["is_free_tier"].as_bool().unwrap_or(false),
    })
}

async fn chat(system: &str, user: &str, model: &str, temperature: f64) -> Result<String, String> {
    let k = key().ok_or(
        "No OpenRouter key. Add one in Settings, or set OPENROUTER_API_KEY for the secure env path.",
    )?;
    let c = client()?;
    let body = json!({
        "model": model,
        "temperature": temperature,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    });
    let r = c
        .post(format!("{BASE}/chat/completions"))
        .header("Authorization", format!("Bearer {k}"))
        .header("HTTP-Referer", "https://github.com/dnh33/forge-studio")
        .header("X-Title", "Forge Studio")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let st = r.status();
    let v: Value = r.json().await.map_err(|e| e.to_string())?;
    if !st.is_success() {
        let msg = v["error"]["message"].as_str().unwrap_or("request failed");
        return Err(format!("OpenRouter {st}: {msg}"));
    }
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "OpenRouter returned no content".to_string())
}

fn strip_fences(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    t.trim_end_matches("```").trim().to_string()
}

/// Turn a short brief into a coherent prompt set. Returns the set as JSON text.
pub async fn ideate(brief: String, model: String, count: u32) -> Result<String, String> {
    let system = format!(
        "You are an art director for a grimdark gothic industrial-sci-fi setting. \
         You write image-generation prompts: concrete, sensory, physical — materials, light, composition, \
         what is in frame. Never generic mood words. Never franchise names or marks. \
         You answer with ONE JSON object and nothing else, exactly this shape:\n\
         {{\"name\":\"<short title>\",\"style\":\"<one dense shared style block>\",\
         \"items\":{{\"<kebab-id>\":{{\"seed\":<int>,\"line\":\"<one item, ~40-70 words>\"}}}}}}\n\
         Produce exactly {count} items with distinct kebab-case ids and distinct seeds. \
         The style block is appended to every item, so it must not repeat an item's subject."
    );
    let user = format!("Brief: {brief}\n\nReturn only the JSON object.");
    let raw = chat(&system, &user, &model, 0.9).await?;
    let body = strip_fences(&raw);
    // validate before handing anything to the UI
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("Model returned invalid JSON: {e}"))?;
    if v["items"].as_object().map(|o| o.is_empty()).unwrap_or(true) {
        return Err("Model returned no items.".into());
    }
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// Rewrite one item line — used by the "sharpen" button on an item row.
pub async fn sharpen(line: String, model: String) -> Result<String, String> {
    let system = "Rewrite the image prompt to be more concrete and visual: specific materials, \
                  light direction, composition, physical detail. Keep the subject and intent. \
                  Return only the rewritten prompt, one paragraph, no preamble.";
    let out = chat(system, &line, &model, 0.7).await?;
    Ok(strip_fences(&out))
}

/// General advisory call — same plumbing as the CLI /advisor, usable in the studio.
pub async fn advise(question: String, context: Option<String>, model: String) -> Result<String, String> {
    let system = "You are a senior technical advisor. Answer directly and concretely. \
                  State uncertainty as uncertainty. No filler, no restating the question.";
    let user = match context {
        Some(c) if !c.trim().is_empty() => format!("Context:\n{c}\n\nQuestion:\n{question}"),
        _ => question,
    };
    chat(system, &user, &model, 0.3).await
}
