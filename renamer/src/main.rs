// Fluora Renamer — the lab's file-naming tool on its own: find the images in a folder (unpacking
// CZIs with Fiji if needed), check the labels, rename the files, and undo if needed. It uses the
// same naming code and the same screens as Fluora's project setup.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use fluora_naming::tasks::{self, Jobs};
use fluora_naming::{merge, rename, Detection};
use http::{Request, Response};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

struct State {
    root: PathBuf,
    jobs: Jobs,
}

type Resp = Response<Vec<u8>>;

fn ok(v: Value) -> Resp {
    Response::builder().header("Content-Type", "application/json").header("Cache-Control", "no-store")
        .body(serde_json::to_vec(&v).unwrap_or_default()).unwrap()
}
fn err(code: u16, msg: &str) -> Resp {
    Response::builder().status(code).header("Content-Type", "application/json").body(serde_json::to_vec(&json!({"error": msg})).unwrap()).unwrap()
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// This person's preferences: initials, optional columns, where Fiji is, the last folder
fn prefs(st: &State) -> Value {
    std::fs::read(st.root.join("prefs.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({}))
}
fn save_prefs(st: &State, v: &Value) {
    let _ = std::fs::create_dir_all(&st.root);
    let _ = std::fs::write(st.root.join("prefs.json"), serde_json::to_vec_pretty(v).unwrap_or_default());
}

fn handle(app: &AppHandle, req: Request<Vec<u8>>) -> Resp {
    let st = app.state::<State>();
    let path = percent_encoding::percent_decode_str(req.uri().path()).decode_utf8_lossy().into_owned();
    if req.method() == http::Method::POST {
        let body: Value = serde_json::from_slice(req.body()).unwrap_or(json!({}));
        return post(app, &st, &path, &body);
    }
    if path == "/api/setup/status" {
        return ok(serde_json::to_value(st.jobs.status()).unwrap());
    }
    let name = if path == "/" { "rename.html" } else { path.trim_start_matches('/') };
    match app.asset_resolver().get(name.to_string()) {
        Some(a) => Response::builder().header("Content-Type", mime(name)).header("Cache-Control", "no-store").body(a.bytes).unwrap(),
        None => Response::builder().status(404).body(b"Not found".to_vec()).unwrap(),
    }
}

fn post(app: &AppHandle, st: &State, path: &str, req: &Value) -> Resp {
    let busy = || err(409, "Still busy with the last step. Wait for it to finish.");
    let started = |b: bool| if b { ok(json!({"ok": true})) } else { busy() };
    let p = prefs(st);
    let fiji = tasks::find_fiji(p["fiji"].as_str().unwrap_or(""));
    match path {
        "/api/pick-folder" => {
            let mut d = app.dialog().file().set_title(req["title"].as_str().unwrap_or("Choose a folder of images"));
            if let Some(start) = req["start"].as_str().filter(|s| Path::new(s).is_dir()) {
                d = d.set_directory(start);
            }
            let picked = d.blocking_pick_folder().and_then(|f| f.into_path().ok()).map(|p| p.to_string_lossy().into_owned());
            ok(json!({"path": picked}))
        }
        "/api/setup/load" => ok(json!({
            "labels": null, "folder": p["lastFolder"].as_str().unwrap_or(""), "initials": p["initials"].as_str().unwrap_or(""),
            "columns": p["columns"].clone(), "fiji": fiji, "fijiJava": fiji.as_deref().and_then(tasks::fiji_java).is_some(),
        })),
        "/api/setup/prefs" => {
            let mut p = p;
            for k in ["initials", "columns", "lastFolder"] {
                if !req[k].is_null() {
                    p[k] = req[k].clone();
                }
            }
            save_prefs(st, &p);
            ok(json!({"ok": true}))
        }
        "/api/setup/detect" => {
            let folder = req["folder"].as_str().unwrap_or("").trim().to_string();
            started(tasks::start_detect(&st.jobs, folder, req["initials"].as_str().unwrap_or("").to_string(), None))
        }
        "/api/setup/merge" => match (serde_json::from_value::<Detection>(req["labels"].clone()), serde_json::from_value::<Detection>(req["fresh"].clone())) {
            (Ok(a), Ok(b)) => ok(serde_json::to_value(merge(&a, &b)).unwrap()),
            _ => err(400, "Labels didn't read"),
        },
        "/api/setup/plan" => match tasks::labels_from(&req["labels"]) {
            Ok(det) => ok(serde_json::to_value(rename::plan_with(&det, req["checkDisk"].as_bool().unwrap_or(false))).unwrap()),
            Err(e) => err(400, &e),
        },
        "/api/setup/rename" => started(tasks::start_rename(&st.jobs, req["labels"].clone())),
        "/api/setup/undo" => started(tasks::start_undo(&st.jobs, req["labels"].clone(), req["stamp"].as_str().unwrap_or("").to_string())),
        "/api/setup/unpack" => {
            let czis = req["czis"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            let dest = req["dest"].as_str().filter(|d| !d.trim().is_empty()).map(|d| (req["root"].as_str().unwrap_or("").to_string(), d.trim().to_string()));
            started(tasks::start_unpack(&st.jobs, st.root.clone(), fiji.unwrap_or_default(), czis, dest))
        }
        "/api/update/check" => ok(fluora_update::check(app)),
        "/api/update/install" => err(500, &fluora_update::install(app)),
        "/api/setup/cancel" => {
            st.jobs.cancel();
            ok(json!({"ok": true}))
        }
        _ => Response::builder().status(404).body(b"Not found".to_vec()).unwrap(),
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(fluora_update::plugin())
        .register_asynchronous_uri_scheme_protocol("sb", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || responder.respond(handle(&app, request)));
        })
        .setup(|app| {
            let root = app.path().app_data_dir()?;
            app.manage(State { root, jobs: Jobs::new() });
            #[cfg(feature = "devserver")]
            devserver(app.handle().clone());
            // Windows serves custom schemes as http://<scheme>.localhost
            let url = if cfg!(windows) { "http://sb.localhost/" } else { "sb://localhost/" };
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::CustomProtocol(url.parse().unwrap()))
                .title("Fluora Renamer")
                .inner_size(1360.0, 880.0)
                .min_inner_size(900.0, 600.0)
                // let the page itself handle drag and drop (dragging slices onto the Board)
                .disable_drag_drop_handler()
                .theme(Some(tauri::Theme::Dark))
                .background_color(tauri::window::Color(11, 12, 15, 255))
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Fluora Renamer couldn't start");
}

/// Testing only: serves the same pages over plain HTTP on 127.0.0.1:47818.
#[cfg(feature = "devserver")]
fn devserver(app: AppHandle) {
    let server = tiny_http::Server::http(("127.0.0.1", 47818)).expect("dev server port in use");
    std::thread::spawn(move || {
        for mut rq in server.incoming_requests() {
            let app = app.clone();
            std::thread::spawn(move || {
                let mut body = vec![];
                let _ = std::io::Read::read_to_end(rq.as_reader(), &mut body);
                let method = if *rq.method() == tiny_http::Method::Post { "POST" } else { "GET" };
                let req = http::Request::builder().method(method).uri(format!("http://localhost{}", rq.url())).body(body).unwrap();
                let resp = handle(&app, req);
                let mut out = tiny_http::Response::from_data(resp.body().clone()).with_status_code(resp.status().as_u16());
                for (k, v) in resp.headers() {
                    if let Ok(h) = tiny_http::Header::from_bytes(k.as_str().as_bytes(), v.as_bytes()) {
                        out.add_header(h);
                    }
                }
                let _ = rq.respond(out);
            });
        }
    });
}
