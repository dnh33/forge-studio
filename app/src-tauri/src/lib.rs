//! Forge Studio — the desktop studio for the forge-images pipeline.
//!
//! The whole backend is a thin, honest wrapper over the GitHub API: read prompt
//! sets, write prompt sets, dispatch the render workflow, stream its status,
//! list the renders and download them. Nothing is cached locally that the
//! repository does not already hold.

mod control;
mod github;
mod openrouter;

use github::{Identity, PromptSet, RenderImage, RunDetail, RunSummary, SetSummary};
use openrouter::{OrKeyCheck, OrModel, OrStatus};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
async fn gh_identity() -> Result<Identity, String> {
    github::identity().await
}

#[tauri::command]
async fn list_sets() -> Result<Vec<SetSummary>, String> {
    github::list_sets().await
}

#[tauri::command]
async fn get_set(slug: String) -> Result<PromptSet, String> {
    github::get_set(slug).await
}

#[tauri::command]
async fn save_set(slug: String, body: String) -> Result<String, String> {
    github::save_set(slug, body).await
}

#[tauri::command]
async fn dispatch_render(
    set: String,
    only: String,
    variants: String,
    steps: String,
    shards: String,
    adhoc: String,
) -> Result<(), String> {
    github::dispatch(github::DispatchOpts {
        set,
        only,
        variants,
        steps,
        shards,
        adhoc,
    })
    .await
}

#[tauri::command]
async fn list_runs(limit: Option<u32>) -> Result<Vec<RunSummary>, String> {
    github::list_runs(limit.unwrap_or(15)).await
}

#[tauri::command]
async fn get_run(id: u64) -> Result<RunDetail, String> {
    github::get_run(id).await
}

#[tauri::command]
async fn list_renders() -> Result<Vec<RenderImage>, String> {
    github::list_renders().await
}

#[tauri::command]
async fn run_outputs(run_id: u64) -> Result<Vec<RenderImage>, String> {
    github::run_outputs(run_id).await
}

#[tauri::command]
async fn download_image(url: String, dir: String) -> Result<String, String> {
    github::download(url, dir).await
}

/// Native folder picker. Returns None when the user cancels.
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

// ------------------------------------------------------------------ OpenRouter

#[tauri::command]
fn or_status() -> OrStatus {
    openrouter::status()
}

#[tauri::command]
fn or_set_key(key: String) -> Result<(), String> {
    openrouter::set_key(key)
}

#[tauri::command]
fn or_clear_key() -> Result<(), String> {
    openrouter::clear_key()
}

#[tauri::command]
fn or_set_model(model: String) -> Result<(), String> {
    openrouter::set_model(model)
}

#[tauri::command]
async fn or_models() -> Result<Vec<OrModel>, String> {
    openrouter::models().await
}

#[tauri::command]
async fn or_verify() -> Result<OrKeyCheck, String> {
    openrouter::verify_key().await
}

#[tauri::command]
async fn or_ideate(brief: String, model: String, count: u32) -> Result<String, String> {
    openrouter::ideate(brief, model, count).await
}

#[tauri::command]
async fn or_sharpen(line: String, model: String) -> Result<String, String> {
    openrouter::sharpen(line, model).await
}

#[tauri::command]
async fn or_advise(
    question: String,
    context: Option<String>,
    model: String,
) -> Result<String, String> {
    openrouter::advise(question, context, model).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|_app| {
            // Full agent control over loopback HTTP. Local-only, token-gated.
            crate::control::start();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            gh_identity,
            list_sets,
            get_set,
            save_set,
            dispatch_render,
            list_runs,
            get_run,
            list_renders,
            run_outputs,
            download_image,
            pick_folder,
            or_status,
            or_set_key,
            or_clear_key,
            or_set_model,
            or_models,
            or_verify,
            or_ideate,
            or_sharpen,
            or_advise,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forge Studio");
}
