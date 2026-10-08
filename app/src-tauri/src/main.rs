// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The same binary is the engine and the window. `--headless` is the engine
    // alone: control plane up, no webview, the process stays alive for as long
    // as the engine should. Anything else is the window.
    if std::env::args()
        .skip(1)
        .any(|a| a == "--headless" || a == "--headless=1")
    {
        forge_studio_lib::headless();
        return;
    }
    forge_studio_lib::run()
}
