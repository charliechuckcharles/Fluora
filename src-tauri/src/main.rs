// Fluora — a desktop app for browsing, marking and snapping slide-scanner images.
//
// The window shows the app's web pages (ui/), served by this program through its own `sb://`
// address, so the pages talk to the Rust code below with plain fetch() calls and nothing listens
// on the network.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod setup;
mod slides;
mod tiffread;
mod snaps;
mod store;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(fluora_update::plugin())
        .register_asynchronous_uri_scheme_protocol("sb", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            // Image work and folder dialogs can take a while, so each request gets its own thread
            std::thread::spawn(move || responder.respond(api::handle(&app, request)));
        })
        .setup(|app| {
            let data_root = app.path().app_data_dir()?;
            app.manage(api::AppState::new(data_root)?);
            #[cfg(feature = "devserver")]
            devserver(app.handle().clone());
            // Windows serves custom schemes as http://<scheme>.localhost
            let url = if cfg!(windows) { "http://sb.localhost/" } else { "sb://localhost/" };
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::CustomProtocol(url.parse().unwrap()))
                .title("Fluora")
                .inner_size(1440.0, 920.0)
                .min_inner_size(960.0, 640.0)
                // let the page itself handle drag and drop (dragging slices onto the Board)
                .disable_drag_drop_handler()
                .theme(Some(tauri::Theme::Dark))
                .background_color(tauri::window::Color(11, 12, 15, 255))
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Fluora couldn't start");
}

/// Testing only (built with --features devserver): serves the same app over plain HTTP on
/// 127.0.0.1, so it can be opened and checked in an ordinary browser.
#[cfg(feature = "devserver")]
fn devserver(app: tauri::AppHandle) {
    let port: u16 = std::env::var("SB_DEV_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(47817);
    let server = tiny_http::Server::http(("127.0.0.1", port)).expect("dev server port in use");
    std::thread::spawn(move || {
        for mut rq in server.incoming_requests() {
            let app = app.clone();
            std::thread::spawn(move || {
                let mut body = vec![];
                let _ = std::io::Read::read_to_end(rq.as_reader(), &mut body);
                let method = if *rq.method() == tiny_http::Method::Post { "POST" } else { "GET" };
                let mut b = http::Request::builder().method(method).uri(format!("http://localhost{}", rq.url()));
                for h in rq.headers() {
                    b = b.header(h.field.as_str().as_str(), h.value.as_str());
                }
                let req = b.body(body).unwrap();
                let resp = api::handle(&app, req);
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
