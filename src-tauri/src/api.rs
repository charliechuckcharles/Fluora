// Answers the app's requests: the pages in ui/, the preview JPEGs, and /api/... for marks,
// settings, contrast, the board and opening files in Fiji or QuPath.
use crate::slides::{self, Builder, Slice};
use crate::setup::{self, Jobs};
use crate::snaps;
use crate::store::{now, read_json, save_json, Store};
use http::{Request, Response, StatusCode};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

pub struct AppState {
    pub store: Store,
    pub builder: Builder,
    pub jobs: Jobs,
}

impl AppState {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        Ok(AppState { store: Store::new(root)?, builder: Builder::new(), jobs: Jobs::new() })
    }
}

type Resp = Response<Vec<u8>>;

fn json_resp(code: u16, v: &Value) -> Resp {
    Response::builder().status(code).header("Content-Type", "application/json").header("Cache-Control", "no-store")
        .body(serde_json::to_vec(v).unwrap_or_default()).unwrap()
}
fn ok(v: Value) -> Resp {
    json_resp(200, &v)
}
fn err(code: u16, msg: &str) -> Resp {
    json_resp(code, &json!({"error": msg}))
}
fn not_found() -> Resp {
    Response::builder().status(StatusCode::NOT_FOUND).body(b"Not found".to_vec()).unwrap()
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

pub fn handle(app: &AppHandle, req: Request<Vec<u8>>) -> Resp {
    let st = app.state::<AppState>();
    let raw = req.uri().path().to_string();
    let path = percent_encoding::percent_decode_str(&raw).decode_utf8_lossy().into_owned();
    if req.method() == http::Method::POST {
        let body: Value = serde_json::from_slice(req.body()).unwrap_or(json!({}));
        return post(app, &st, &path, body);
    }
    match path.as_str() {
        "/api/slides" => ok(index(&st)),
        "/api/board" => {
            let dir = st.store.current_dir();
            ok(read_json(&dir.join("board.json"), json!({"groups": [], "cards": [], "view": {"x": 40, "y": 40, "k": 1}})))
        }
        "/api/status" => {
            let dir = st.store.current_dir();
            ok(json!({"build": build_status(&st), "labConnected": folder_ok(&dir),
                "builtAt": read_json(&dir.join("index.json"), json!({}))["builtAt"]}))
        }
        "/api/snaps" => ok(snaps::list(&st.store.current_dir())),
        "/api/setup/status" => ok(serde_json::to_value(st.jobs.status()).unwrap()),
        p if p.starts_with("/snap/") => {
            // A snap's PNG or its thumbnail, only from inside the snaps folder
            let rel = p.trim_start_matches("/snap/");
            if rel.split('/').any(|part| part == ".." || part.is_empty()) {
                return not_found();
            }
            match std::fs::read(snaps::dir(&st.store.current_dir()).join(rel)) {
                Ok(b) => Response::builder().header("Content-Type", mime(rel)).header("Cache-Control", "max-age=3600").body(b).unwrap(),
                Err(_) => not_found(),
            }
        }
        p if p.starts_with("/img/") => {
            let name = Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match std::fs::read(st.store.current_dir().join("images").join(&name)) {
                Ok(b) => Response::builder().header("Content-Type", mime(&name)).header("Cache-Control", "max-age=60").body(b).unwrap(),
                Err(_) => not_found(),
            }
        }
        p => {
            let name = if p == "/" { "index.html" } else { p.trim_start_matches('/') };
            match app.asset_resolver().get(name.to_string()) {
                Some(a) => Response::builder().header("Content-Type", mime(name)).header("Cache-Control", "no-store").body(a.bytes).unwrap(),
                None => not_found(),
            }
        }
    }
}

// ---------------------------------------------------------------- settings

const SETTING_KEYS: [&str; 4] = ["imageFolder", "projectName", "conditions", "expectedSlices"];

fn load_settings(st: &AppState, dir: &Path) -> Value {
    let mut s = read_json(&dir.join("settings.json"), json!({}));
    if !s.is_object() {
        s = json!({});
    }
    for (k, d) in [("imageFolder", json!("")), ("projectName", json!("")), ("conditions", json!({})), ("expectedSlices", json!({}))] {
        if s.get(k).is_none_or(|v| v.is_null()) {
            s[k] = d;
        }
    }
    // Where Fiji and QuPath are belongs to this computer, not the project
    let p = st.store.profile();
    s["fiji"] = p["fiji"].clone();
    s["qupath"] = p["qupath"].clone();
    s
}

fn image_folder(dir: &Path) -> String {
    read_json(&dir.join("settings.json"), json!({}))["imageFolder"].as_str().unwrap_or("").to_string()
}

fn folder_ok(dir: &Path) -> Value {
    let f = image_folder(dir);
    if f.is_empty() { Value::Null } else { json!(Path::new(&f).is_dir()) }
}

fn build_status(st: &AppState) -> Value {
    let b = st.builder.status();
    // A build for another project shows as idle here
    if b.project.as_deref().is_some_and(|p| p != st.store.current_id()) {
        return json!({"phase": "idle", "done": 0, "total": 0, "message": "", "error": null});
    }
    serde_json::to_value(b).unwrap()
}

fn index(st: &AppState) -> Value {
    let dir = st.store.current_dir();
    let mut data = read_json(&dir.join("index.json"), json!({"slices": [], "builtAt": null}));
    let settings = load_settings(st, &dir);
    if let Some(list) = data["slices"].as_array_mut() {
        for s in list {
            // Condition names come from Settings, so changing them needs no refresh
            let code = s["cond"].as_str().unwrap_or("").to_string();
            let name = settings["conditions"][&code].as_str().map(String::from).unwrap_or(code);
            s["condition"] = json!(if name.is_empty() { "No condition".to_string() } else { name });
        }
    }
    data["marks"] = read_json(&dir.join("marks.json"), json!({}));
    data["contrast"] = read_json(&dir.join("contrast.json"), json!({}));
    data["build"] = build_status(st);
    data["labConnected"] = folder_ok(&dir);
    data["hasLabels"] = json!(dir.join("labels.json").exists());
    data["stainColors"] = slides::stain_colors_hex(&dir);
    data["apps"] = json!({"fiji": find_fiji(st), "qupath": find_qupath(st)});
    data["settings"] = settings;
    data["profile"] = st.store.profile();
    data["projects"] = projects_with_progress(st);
    data["project"] = json!(st.store.current_id());
    data
}

/// The project list, each with how many slices it has and how many have a mark (for its flower).
fn projects_with_progress(st: &AppState) -> Value {
    let mut pr = st.store.projects();
    if let Some(list) = pr["projects"].as_array_mut() {
        for p in list.iter_mut() {
            let dir = st.store.project_dir(p["id"].as_str().unwrap_or(""));
            let index = read_json(&dir.join("index.json"), json!({}));
            let marks = read_json(&dir.join("marks.json"), json!({}));
            let ids: Vec<&str> = index["slices"].as_array().map(|a| a.iter().filter_map(|s| s["id"].as_str()).collect()).unwrap_or_default();
            let reviewed = ids.iter().filter(|id| marks[**id]["status"].as_str().is_some_and(|x| !x.is_empty())).count();
            p["total"] = json!(ids.len());
            p["reviewed"] = json!(reviewed);
        }
    }
    pr
}

// ---------------------------------------------------------------- POST

fn post(app: &AppHandle, st: &AppState, path: &str, req: Value) -> Resp {
    let dir = st.store.current_dir();
    match path {
        "/api/update/check" => ok(fluora_update::check(app)),
        "/api/update/install" => err(500, &fluora_update::install(app)),
        "/api/marks" => {
            let Some(id) = req["id"].as_str() else { return err(400, "No image id") };
            let _g = st.store.lock();
            let f = dir.join("marks.json");
            let mut marks = read_json(&f, json!({}));
            let mut m = marks.get(id).cloned().filter(|v| v.is_object()).unwrap_or(json!({}));
            // status: good / ok / bad; note; pick: picked for the Bouquet
            for k in ["status", "note", "pick"] {
                if let Some(v) = req.get(k) {
                    m[k] = v.clone();
                }
            }
            if m["pick"] == json!(false) {
                m.as_object_mut().map(|o| { o.remove("pick"); o.remove("pickedAt") });
            } else if req["pick"] == json!(true) {
                m["pickedAt"] = json!(now());
            }
            let empty = |v: &Value| v.is_null() || v.as_str() == Some("");
            if empty(&m["status"]) && empty(&m["note"]) && m["pick"] != json!(true) {
                marks.as_object_mut().map(|o| o.remove(id));
            } else {
                m["updated"] = json!(now());
                let name = st.store.profile()["name"].clone();
                if name.as_str().is_some_and(|n| !n.is_empty()) {
                    m["by"] = name;
                }
                marks[id] = m;
            }
            if let Err(e) = save_json(&f, &marks) {
                return err(500, &e.to_string());
            }
            ok(json!({"ok": true, "marks": marks}))
        }
        "/api/contrast" => {
            {
                let _g = st.store.lock();
                let f = dir.join("contrast.json");
                let mut contrast = read_json(&f, json!({}));
                if let Some(changes) = req["contrast"].as_object() {
                    for (stain, c) in changes {
                        let mut cur = contrast.get(stain).cloned().filter(|v| v.is_object()).unwrap_or(json!({}));
                        if let Some(o) = c.as_object() {
                            for (k, v) in o {
                                cur[k] = v.clone();
                            }
                        }
                        cur["auto"] = json!(c["auto"].as_bool().unwrap_or(false));
                        contrast[stain] = cur;
                    }
                }
                let _ = save_json(&f, &contrast);
            }
            let started = st.builder.start(st.store.current_id(), dir.clone(), image_folder(&dir), true);
            ok(json!({"ok": true, "started": started}))
        }
        "/api/board" => {
            if !req["groups"].is_array() || !req["cards"].is_array() {
                return err(400, "Not a board");
            }
            let _g = st.store.lock();
            let v = json!({"groups": req["groups"], "cards": req["cards"], "view": if req["view"].is_object() { req["view"].clone() } else { json!({}) }, "saved": now()});
            match save_json(&dir.join("board.json"), &v) {
                Ok(()) => ok(json!({"ok": true})),
                Err(e) => err(500, &e.to_string()),
            }
        }
        "/api/settings" => {
            let _g = st.store.lock();
            let mut s = read_json(&dir.join("settings.json"), json!({}));
            if !s.is_object() {
                s = json!({});
            }
            for k in SETTING_KEYS {
                if let Some(v) = req.get(k) {
                    s[k] = v.clone();
                }
            }
            if let Some(f) = s["imageFolder"].as_str() {
                s["imageFolder"] = json!(expand_home(f.trim()));
            }
            let _ = save_json(&dir.join("settings.json"), &s);
            let mut p = st.store.profile();
            for k in ["fiji", "qupath"] {
                if let Some(v) = req[k].as_str() {
                    p[k] = json!(v.trim());
                }
            }
            let _ = st.store.save_profile(&p);
            // The project's name in the project list follows the project name setting
            if let Some(name) = req["projectName"].as_str().filter(|n| !n.trim().is_empty()) {
                let mut pr = st.store.projects();
                let id = st.store.current_id();
                if let Some(list) = pr["projects"].as_array_mut() {
                    for x in list.iter_mut().filter(|x| x["id"] == json!(id)) {
                        x["name"] = json!(name.trim());
                    }
                }
                let _ = st.store.save_projects(&pr);
            }
            drop(_g);
            ok(json!({"ok": true, "settings": load_settings(st, &dir), "folderFound": folder_ok(&dir)}))
        }
        "/api/rebuild" => ok(json!({"ok": true, "started": st.builder.start(st.store.current_id(), dir.clone(), image_folder(&dir), false)})),
        "/api/pick-folder" => {
            // The system's own folder picker, so nobody has to copy and paste paths
            let mut d = app.dialog().file().set_title(req["title"].as_str().unwrap_or("Choose your image folder"));
            if let Some(start) = req["start"].as_str().filter(|s| Path::new(s).is_dir()) {
                d = d.set_directory(start);
            }
            let picked = d.blocking_pick_folder().and_then(|f| f.into_path().ok()).map(|p| p.to_string_lossy().into_owned());
            ok(json!({"path": picked}))
        }
        "/api/profile" => {
            let _g = st.store.lock();
            let mut p = st.store.profile();
            if let Some(n) = req["name"].as_str() {
                p["name"] = json!(n.trim());
            }
            // Which intro tours this person has seen
            if let Some(seen) = req["seen"].as_object() {
                if !p["seen"].is_object() {
                    p["seen"] = json!({});
                }
                for (k, v) in seen {
                    p["seen"][k] = v.clone();
                }
            }
            match st.store.save_profile(&p) {
                Ok(()) => ok(json!({"ok": true, "profile": p})),
                Err(e) => err(500, &e.to_string()),
            }
        }
        "/api/projects" => projects(st, &req),
        "/api/snap-region" => snap_region(st, &dir, &req),
        "/api/snap-save" => snap_save(st, &req),
        "/api/snaps" => snap_action(st, &dir, &req),
        "/api/log" => {
            // Lets the app window report problems to a log file, since it has no console to read
            let line = format!("{:.0} {}\n", now(), req["msg"].as_str().unwrap_or(""));
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(st.store.root.join("app.log")) {
                let _ = f.write_all(line.as_bytes());
            }
            ok(json!({"ok": true}))
        }
        "/api/open" => open(st, &dir, &req),
        p if p.starts_with("/api/setup/") => setup_post(st, &dir, &p["/api/setup/".len()..], &req),
        _ => not_found(),
    }
}

/// The project setup steps (see setup.rs)
fn setup_post(st: &AppState, dir: &Path, what: &str, req: &Value) -> Resp {
    let busy = || err(409, "Fluora is still busy with the last step. Wait for it to finish.");
    match what {
        "load" => {
            let mut v = setup::load(dir);
            v["folder"] = json!(image_folder(dir));
            v["fiji"] = find_fiji(st);
            v["fijiJava"] = json!(find_fiji(st).as_str().and_then(setup::fiji_java).is_some());
            let p = st.store.profile();
            v["initials"] = json!(p["initials"].as_str().filter(|s| !s.is_empty()).map(String::from)
                .unwrap_or_else(|| setup::initials_of(p["name"].as_str().unwrap_or(""))));
            v["columns"] = p["labelColumns"].clone();
            ok(v)
        }
        "detect" => {
            let folder = expand_home(req["folder"].as_str().unwrap_or("").trim());
            let initials = req["initials"].as_str().unwrap_or("").to_string();
            let saved = if req["merge"].as_bool().unwrap_or(false) {
                serde_json::from_value(read_json(&dir.join("labels.json"), Value::Null)).ok()
            } else {
                None
            };
            if setup::start_detect(&st.jobs, folder, initials, saved) { ok(json!({"ok": true})) } else { busy() }
        }
        // Brings labels (as they are on screen now) up to date with a fresh look at the folder
        "merge" => match (serde_json::from_value::<fluora_naming::Detection>(req["labels"].clone()), serde_json::from_value::<fluora_naming::Detection>(req["fresh"].clone())) {
            (Ok(a), Ok(b)) => ok(serde_json::to_value(fluora_naming::merge(&a, &b)).unwrap()),
            _ => err(400, "Labels didn't read"),
        },
        "plan" => match setup::plan(&req["labels"], req["checkDisk"].as_bool().unwrap_or(false)) {
            Ok(v) => ok(v),
            Err(e) => err(400, &e),
        },
        "rename" => if setup::start_rename(&st.jobs, req["labels"].clone()) { ok(json!({"ok": true})) } else { busy() },
        "undo" => {
            let stamp = req["stamp"].as_str().unwrap_or("").to_string();
            if setup::start_undo(&st.jobs, req["labels"].clone(), stamp) { ok(json!({"ok": true})) } else { busy() }
        }
        "channels" => match setup::channels(&req["labels"], dir) {
            Ok(v) => ok(v),
            Err(e) => err(400, &e),
        },
        "unpack" => {
            let czis: Vec<String> = req["czis"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            let fiji = find_fiji(st).as_str().unwrap_or("").to_string();
            let dest = req["dest"].as_str().filter(|d| !d.trim().is_empty()).map(|d| (req["root"].as_str().unwrap_or("").to_string(), expand_home(d.trim())));
            if setup::start_unpack(&st.jobs, st.store.root.clone(), fiji, czis, dest) { ok(json!({"ok": true})) } else { busy() }
        }
        "cancel" => {
            st.jobs.cancel();
            ok(json!({"ok": true}))
        }
        "save" => {
            let folder = expand_home(req["folder"].as_str().unwrap_or("").trim());
            if let Err(e) = setup::save(dir, &req["labels"], &req["channels"]) {
                return err(500, &e);
            }
            {
                let _g = st.store.lock();
                let f = dir.join("settings.json");
                let mut s = read_json(&f, json!({}));
                s["imageFolder"] = json!(folder);
                let _ = save_json(&f, &s);
                // Remember the initials and optional columns this person uses, for next time
                let mut p = st.store.profile();
                if let Some(i) = req["initials"].as_str().filter(|x| !x.is_empty()) {
                    p["initials"] = json!(i);
                }
                if req["columns"].is_object() {
                    p["labelColumns"] = req["columns"].clone();
                }
                let _ = st.store.save_profile(&p);
            }
            let rerender = req["rerenderOnly"].as_bool().unwrap_or(false) && dir.join("index.json").exists();
            let started = st.builder.start(st.store.current_id(), dir.to_path_buf(), folder, rerender);
            ok(json!({"ok": true, "started": started}))
        }
        _ => not_found(),
    }
}

/// Make, open, rename or remove a project. Each project is its own folder of settings, marks,
/// board and previews, so several image sets can be browsed side by side.
fn projects(st: &AppState, req: &Value) -> Resp {
    let _g = st.store.lock();
    let mut pr = st.store.projects();
    let action = req["action"].as_str().unwrap_or("");
    let id = req["id"].as_str().unwrap_or("").to_string();
    let exists = |pr: &Value, id: &str| pr["projects"].as_array().unwrap().iter().any(|x| x["id"] == json!(id));
    match action {
        "create" => {
            let name = req["name"].as_str().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("New project").to_string();
            let folder = expand_home(req["imageFolder"].as_str().unwrap_or("").trim());
            let new_id = st.store.new_project_id();
            let dir = st.store.project_dir(&new_id);
            let settings = json!({"imageFolder": folder, "projectName": name,
                "conditions": if req["conditions"].is_object() { req["conditions"].clone() } else { json!({}) },
                "expectedSlices": if req["expectedSlices"].is_object() { req["expectedSlices"].clone() } else { json!({}) }});
            if let Err(e) = save_json(&dir.join("settings.json"), &settings) {
                return err(500, &e.to_string());
            }
            pr["projects"].as_array_mut().unwrap().push(json!({"id": new_id, "name": name, "created": now()}));
            pr["current"] = json!(new_id);
            let _ = st.store.save_projects(&pr);
            // Start finding slides straight away when the folder is reachable
            let started = !folder.is_empty() && Path::new(&folder).is_dir()
                && st.builder.start(new_id.clone(), dir, folder.clone(), false);
            ok(json!({"ok": true, "id": new_id, "started": started, "folderFound": folder.is_empty() || Path::new(&folder).is_dir()}))
        }
        "switch" => {
            if !exists(&pr, &id) {
                return err(404, "No such project");
            }
            pr["current"] = json!(id);
            let _ = st.store.save_projects(&pr);
            ok(json!({"ok": true}))
        }
        "rename" => {
            let Some(name) = req["name"].as_str().map(str::trim).filter(|n| !n.is_empty()) else { return err(400, "Give the project a name") };
            if !exists(&pr, &id) {
                return err(404, "No such project");
            }
            for x in pr["projects"].as_array_mut().unwrap().iter_mut().filter(|x| x["id"] == json!(id)) {
                x["name"] = json!(name);
            }
            let _ = st.store.save_projects(&pr);
            let sf = st.store.project_dir(&id).join("settings.json");
            let mut s = read_json(&sf, json!({}));
            s["projectName"] = json!(name);
            let _ = save_json(&sf, &s);
            ok(json!({"ok": true}))
        }
        "remove" => {
            if !exists(&pr, &id) {
                return err(404, "No such project");
            }
            if st.builder.status().phase != "done" && st.builder.status().phase != "idle" && st.builder.status().phase != "error"
                && st.builder.status().project.as_deref() == Some(id.as_str()) {
                return err(409, "This project is still making its previews. Try again when that's finished.");
            }
            // Never deleted outright: the folder moves to removed/, so it can be brought back
            let removed = st.store.root.join("removed");
            let _ = std::fs::create_dir_all(&removed);
            let src = st.store.project_dir(&id);
            if src.exists() {
                if let Err(e) = std::fs::rename(&src, removed.join(format!("{id}-{}", now() as u64))) {
                    return err(500, &e.to_string());
                }
            }
            pr["projects"].as_array_mut().unwrap().retain(|x| x["id"] != json!(id));
            if pr["current"] == json!(id) {
                pr["current"] = pr["projects"].as_array().unwrap().first().map(|x| x["id"].clone()).unwrap_or(Value::Null);
            }
            let _ = st.store.save_projects(&pr);
            ok(json!({"ok": true}))
        }
        _ => err(400, "Unknown project action"),
    }
}

fn expand_home(p: &str) -> String {
    let p = p.trim_matches(|c| c == '"' || c == '\'');
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(h) = home() {
            return h.join(rest).to_string_lossy().into_owned();
        }
    }
    p.to_string()
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

// ---------------------------------------------------------------- Fiji, QuPath, Finder

fn find_app(chosen: &str, patterns: &[&str]) -> Value {
    if !chosen.is_empty() {
        return json!(chosen);
    }
    let mut roots: Vec<PathBuf> = vec![PathBuf::from("/Applications")];
    if let Some(h) = home() {
        roots.extend([h.join("Applications"), h.clone(), h.join("Desktop"), h.join("Downloads")]);
    }
    for var in ["ProgramFiles", "LOCALAPPDATA"] {
        if let Some(v) = std::env::var_os(var) {
            roots.push(PathBuf::from(v));
        }
    }
    let mut hits: Vec<String> = vec![];
    for r in roots.iter().filter(|r| r.is_dir()) {
        for p in patterns {
            let pat = format!("{}/{}", glob::Pattern::escape(&r.to_string_lossy()), p);
            if let Ok(g) = glob::glob(&pat) {
                hits.extend(g.filter_map(|x| x.ok()).map(|x| x.to_string_lossy().into_owned()));
            }
        }
    }
    hits.sort();
    hits.pop().map(Value::String).unwrap_or(Value::Null)
}

fn find_fiji(st: &AppState) -> Value {
    let chosen = st.store.profile()["fiji"].as_str().unwrap_or("").to_string();
    // On Windows Fiji is a folder (often still called Fiji.app) holding the program
    if cfg!(target_os = "macos") {
        find_app(&chosen, &["Fiji.app", "Fiji/Fiji.app", "Fiji*.app"])
    } else {
        find_app(&chosen, &["Fiji*/fiji-windows-x64.exe", "Fiji*/ImageJ-win64.exe", "*/Fiji*/fiji-windows-x64.exe", "*/Fiji*/ImageJ-win64.exe"])
    }
}

fn find_qupath(st: &AppState) -> Value {
    let chosen = st.store.profile()["qupath"].as_str().unwrap_or("").to_string();
    if cfg!(target_os = "macos") {
        find_app(&chosen, &["QuPath*.app"])
    } else {
        find_app(&chosen, &["QuPath*/QuPath*.exe", "Programs/QuPath*/QuPath*.exe"])
    }
}

fn open_with(app: &Value, files: &[String]) -> std::io::Result<()> {
    let Some(app) = app.as_str() else { return Err(std::io::ErrorKind::NotFound.into()) };
    if cfg!(target_os = "macos") {
        Command::new("open").arg("-a").arg(app).args(files).spawn()?;
    } else {
        Command::new(app).args(files).spawn()?;
    }
    Ok(())
}

fn reveal(path: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("explorer").raw_arg(format!("/select,\"{}\"", path.replace('/', "\\"))).spawn();
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(Path::new(path).parent().unwrap_or(Path::new("/"))).spawn();
}

fn find_slice(dir: &Path, id: &str) -> Option<Slice> {
    let slices: Vec<Slice> = serde_json::from_value(read_json(&dir.join("index.json"), json!({}))["slices"].clone()).unwrap_or_default();
    slices.into_iter().find(|s| s.id == id)
}

// ---------------------------------------------------------------- snaps

/// The pixels for a snap: the zoomed-in region, as sharp as the scanner's files allow.
fn snap_region(_st: &AppState, dir: &Path, req: &Value) -> Resp {
    let Some(s) = find_slice(dir, req["id"].as_str().unwrap_or("")) else { return err(404, "That image isn’t in this project") };
    let f = |k: &str| req[k].as_f64().unwrap_or(0.0);
    let frac = [f("fx"), f("fy"), f("fw").max(1e-4), f("fh").max(1e-4)];
    let contrast = read_json(&dir.join("contrast.json"), json!({}));
    let channel = req["channel"].as_str().unwrap_or("merged");
    match snaps::region(dir, &s, &contrast, frac, channel, f("want").max(200.0), req["bright"].as_f64().unwrap_or(1.0) as f32) {
        Ok(r) => Response::builder().header("Content-Type", "image/png").header("Cache-Control", "no-store")
            .header("X-Snap-Level", r.level.to_string())
            .header("X-Snap-Um", r.um_per_px.map(|u| u.to_string()).unwrap_or_default())
            .header("X-Snap-Source", r.source)
            .header("X-Snap-Size", format!("{}x{}", r.w, r.h))
            .body(r.png).unwrap(),
        Err(e) => err(500, &format!("Couldn't make the snap: {e}")),
    }
}

/// Saves a finished snap: the PNG (base64) and its details.
fn snap_save(st: &AppState, req: &Value) -> Resp {
    use base64::Engine;
    let dir = st.store.current_dir();
    let meta = req["meta"].clone();
    let Ok(png) = base64::engine::general_purpose::STANDARD.decode(req["png"].as_str().unwrap_or("")) else {
        return err(400, "The snap image didn't arrive");
    };
    let by = st.store.profile()["name"].as_str().unwrap_or("").to_string();
    let pname = load_settings(st, &dir)["projectName"].as_str().unwrap_or("").to_string();
    let _g = st.store.lock();
    match snaps::save(&dir, &png, meta, &by, &pname) {
        Ok(rec) => ok(json!({"ok": true, "snap": rec})),
        Err(e) => err(500, &format!("Couldn't save the snap: {e}")),
    }
}

fn snap_action(_st: &AppState, dir: &Path, req: &Value) -> Resp {
    let id = req["id"].as_str().unwrap_or("");
    let file_of = |id: &str| snaps::list(dir).as_array().unwrap().iter().find(|s| s["id"] == json!(id))
        .and_then(|s| s["file"].as_str().map(|f| snaps::dir(dir).join(f)));
    match req["action"].as_str().unwrap_or("") {
        "update" => match snaps::update(dir, id, req) {
            Ok(s) => ok(json!({"ok": true, "snap": s})),
            Err(e) => err(400, &e),
        },
        "remove" => match snaps::remove(dir, id) {
            Ok(()) => ok(json!({"ok": true})),
            Err(e) => err(500, &e),
        },
        "reveal" => match file_of(id) {
            Some(p) => { reveal(&p.to_string_lossy()); ok(json!({"ok": true})) }
            None => err(404, "No such snap"),
        },
        "folder" => {
            let d = snaps::dir(dir);
            let _ = std::fs::create_dir_all(&d);
            open_folder(&d);
            ok(json!({"ok": true}))
        }
        "fiji" => match file_of(id) {
            Some(p) => match open_with(&find_fiji(_st), &[p.to_string_lossy().into_owned()]) {
                Ok(()) => ok(json!({"ok": true})),
                Err(_) => err(400, "Couldn't find Fiji. Install it, or set where it is in Settings."),
            },
            None => err(404, "No such snap"),
        },
        _ => err(400, "Unknown snap action"),
    }
}

fn open_folder(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(path).spawn();
    #[cfg(windows)]
    let _ = Command::new("explorer").arg(path).spawn();
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(path).spawn();
}

fn open(st: &AppState, dir: &Path, req: &Value) -> Resp {
    let id = req["id"].as_str().unwrap_or("");
    let Some(s) = find_slice(dir, id) else { return err(404, "That image isn’t in this project") };
    let s = &s;
    let Some(full) = s.full_res().cloned() else { return err(400, "Nothing to open") };
    if !Path::new(&full).exists() {
        return err(400, "Can't reach the full-resolution file. If it's on the lab drive, check the VPN and that the drive is connected.");
    }
    let what = req["what"].as_str().unwrap_or("");
    match what {
        "fiji" | "fiji-channels" => {
            // The full-resolution exports when they exist, otherwise the scanner's own R1 TIFF
            let ex = Path::new(&s.folder).join(slides::EXPORT_FOLDER);
            let name = Path::new(&full).file_name().unwrap().to_string_lossy().into_owned();
            let mut files: Vec<String> = if what == "fiji" {
                vec![ex.join("merged").join(name.replace("G[R1]", "G[MERGED]")).to_string_lossy().into_owned()]
            } else {
                s.channels.iter().map(|c| ex.join("channels").join(name.replace("G[R1]", &format!("G[{}]", c.stain))).to_string_lossy().into_owned()).collect()
            };
            if files.is_empty() || !files.iter().all(|f| Path::new(f).exists()) {
                files = vec![full];
            }
            if open_with(&find_fiji(st), &files).is_err() {
                return err(400, "Couldn't find Fiji. Install it, or set where it is in Settings.");
            }
        }
        "finder" => reveal(&full),
        "qupath" if s.czi.is_some() => {
            if open_with(&find_qupath(st), &[s.czi.clone().unwrap()]).is_err() {
                return err(400, "Couldn't find QuPath. Install it, or set where it is in Settings.");
            }
        }
        _ => return err(400, "Nothing to open"),
    }
    ok(json!({"ok": true}))
}
