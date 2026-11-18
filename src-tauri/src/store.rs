// Where Fluora keeps things, all on this computer, in the system's app-data folder
// (macOS: ~/Library/Application Support/<app id>/, Windows: %APPDATA%\<app id>\):
//
//   profile.json            your name ("your account") and where Fiji / QuPath are
//   projects.json           your projects and which one is open
//   projects/<id>/          one folder per project:
//       settings.json       image folder, project name, condition codes, expected slice counts
//       index.json          the slices found in the image folder
//       marks.json          good / okay / not good, and notes
//       contrast.json       brightness & contrast per stain
//       board.json          the Board tab
//       cache/              preview-size TIFFs copied from the image folder
//       images/             the JPEGs the app shows
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub fn read_json(path: &Path, default: Value) -> Value {
    fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(default)
}

/// Writes to a temporary file first, so a crash never leaves half a file behind.
pub fn save_json(path: &Path, v: &Value) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    fs::rename(&tmp, path)
}

pub struct Store {
    pub root: PathBuf,
    lock: Mutex<()>,
}

impl Store {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(root.join("projects"))?;
        Ok(Store { root, lock: Mutex::new(()) })
    }

    /// Serialises read-modify-write of the JSON files.
    pub fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn profile(&self) -> Value {
        let mut p = read_json(&self.root.join("profile.json"), json!({}));
        for k in ["name", "fiji", "qupath"] {
            if !p[k].is_string() {
                p[k] = json!("");
            }
        }
        p
    }

    pub fn save_profile(&self, p: &Value) -> std::io::Result<()> {
        save_json(&self.root.join("profile.json"), p)
    }

    pub fn projects(&self) -> Value {
        let mut p = read_json(&self.root.join("projects.json"), json!({"current": null, "projects": []}));
        if !p["projects"].is_array() {
            p["projects"] = json!([]);
        }
        p
    }

    pub fn save_projects(&self, p: &Value) -> std::io::Result<()> {
        save_json(&self.root.join("projects.json"), p)
    }

    /// The open project's id, making a first project if there are none yet.
    pub fn current_id(&self) -> String {
        let mut p = self.projects();
        let ids: Vec<String> = p["projects"].as_array().unwrap().iter()
            .filter_map(|x| x["id"].as_str().map(String::from)).collect();
        if let Some(cur) = p["current"].as_str() {
            if ids.iter().any(|i| i == cur) {
                return cur.to_string();
            }
        }
        if let Some(first) = ids.first() {
            p["current"] = json!(first);
            let _ = self.save_projects(&p);
            return first.clone();
        }
        let id = self.new_project_id();
        p["projects"] = json!([{"id": id, "name": "My slides", "created": now()}]);
        p["current"] = json!(id);
        let _ = self.save_projects(&p);
        id
    }

    pub fn new_project_id(&self) -> String {
        let base = format!("p{}", (now() * 1000.0) as u64);
        let mut id = base.clone();
        let mut n = 1;
        while self.root.join("projects").join(&id).exists() {
            n += 1;
            id = format!("{base}-{n}");
        }
        id
    }

    pub fn project_dir(&self, id: &str) -> PathBuf {
        self.root.join("projects").join(id)
    }

    pub fn current_dir(&self) -> PathBuf {
        let d = self.project_dir(&self.current_id());
        let _ = fs::create_dir_all(&d);
        d
    }
}
