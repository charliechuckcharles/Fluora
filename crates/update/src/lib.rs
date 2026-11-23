//! In-app updates, shared by Fluora and Fluora Renamer.
//!
//! When the app starts, its page asks `check()`: is there a newer published release on GitHub?
//! If the person says yes, `install()` downloads it, checks its signature against the public key
//! in tauri.conf.json, swaps the app for the new one and restarts it. Nothing is downloaded
//! unless the person agrees.
use serde_json::{json, Value};
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

pub fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R, tauri_plugin_updater::Config> {
    tauri_plugin_updater::Builder::new().build()
}

/// `{"current": "0.2.0", "version": "0.3.0", "notes": "…"}` when a newer version is out,
/// `{"current": "0.2.0", "version": null}` when this is the newest (or GitHub can't be reached).
pub fn check(app: &AppHandle) -> Value {
    let current = app.package_info().version.to_string();
    let found = tauri::async_runtime::block_on(async { app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string()) });
    match found {
        Ok(Some(u)) => json!({"current": current, "version": u.version, "notes": u.body.unwrap_or_default()}),
        Ok(None) => json!({"current": current, "version": null}),
        Err(e) => json!({"current": current, "version": null, "error": e}),
    }
}

/// Downloads and installs the newest version, then restarts the app. Only returns if it failed.
pub fn install(app: &AppHandle) -> String {
    let done = tauri::async_runtime::block_on(async {
        let updater = app.updater().map_err(|e| e.to_string())?;
        match updater.check().await.map_err(|e| e.to_string())? {
            Some(u) => u.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string()),
            None => Err("This is already the newest version.".to_string()),
        }
    });
    match done {
        Ok(()) => app.restart(),
        Err(e) => e,
    }
}
