//! The slow jobs shared by Fluora's setup and the standalone renamer: looking through a folder,
//! renaming (and undoing), and unpacking CZIs with Fiji. Each runs in the background as one job
//! at a time, and the app polls `Jobs::status` for progress.
use crate::{detect, merge, rename, Detection};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub kind: String,
    /// "idle", "running", "done" or "error"
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub message: String,
    pub error: Option<String>,
    pub result: Value,
}

pub struct Jobs {
    state: Arc<Mutex<Job>>,
    child: Arc<Mutex<Option<Child>>>,
    cancel: Arc<AtomicBool>,
}

impl Jobs {
    pub fn new() -> Self {
        Jobs {
            state: Arc::new(Mutex::new(Job { kind: String::new(), phase: "idle".into(), done: 0, total: 0, message: String::new(),
                error: None, result: Value::Null })),
            child: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn status(&self) -> Job {
        self.state.lock().unwrap().clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }

    /// Runs `work` in the background unless another job is running. `work` gets a progress
    /// callback (done, total, message) and returns the job's result.
    pub fn start<F>(&self, kind: &str, work: F) -> bool
    where
        F: FnOnce(&dyn Fn(usize, usize, &str), &Arc<AtomicBool>, &Arc<Mutex<Option<Child>>>) -> Result<Value, String> + Send + 'static,
    {
        {
            let mut s = self.state.lock().unwrap();
            if s.phase == "running" {
                return false;
            }
            *s = Job { kind: kind.into(), phase: "running".into(), done: 0, total: 0, message: String::new(), error: None, result: Value::Null };
        }
        self.cancel.store(false, Ordering::SeqCst);
        let (state, cancel, child) = (self.state.clone(), self.cancel.clone(), self.child.clone());
        std::thread::spawn(move || {
            let st2 = state.clone();
            let progress = move |done: usize, total: usize, msg: &str| {
                let mut s = st2.lock().unwrap();
                s.done = done;
                s.total = total;
                s.message = msg.to_string();
            };
            let r = work(&progress, &cancel, &child);
            let mut s = state.lock().unwrap();
            match r {
                Ok(v) => {
                    s.phase = "done".into();
                    s.result = v;
                }
                Err(e) => {
                    s.phase = "error".into();
                    s.error = Some(e);
                }
            }
        });
        true
    }
}

pub fn labels_from(v: &Value) -> Result<Detection, String> {
    serde_json::from_value(v.clone()).map_err(|e| format!("Labels didn't read: {e}"))
}

/// "Ada Lovelace" → "AL"
pub fn initials_of(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(3).collect::<String>().to_uppercase()
}

// ---------------------------------------------------------------- finding images

/// Looks through `folder`. With `saved` labels for the same folder, the result keeps everything
/// in them and marks what's new.
pub fn start_detect(jobs: &Jobs, folder: String, initials: String, saved: Option<Detection>) -> bool {
    jobs.start("detect", move |progress, _cancel, _child| {
        let root = PathBuf::from(&folder);
        if !root.is_dir() {
            return Err(format!("Can't find {folder}. If it's on the lab drive, connect the VPN and the drive, then try again."));
        }
        let n = std::sync::atomic::AtomicUsize::new(0);
        let det = detect(&root, &initials, &|msg| {
            let k = n.fetch_add(1, Ordering::Relaxed) + 1;
            progress(k, 0, msg);
        });
        let det = match saved.filter(|s| Path::new(&s.root) == root.as_path()) {
            Some(s) => merge(&s, &det),
            None => det,
        };
        Ok(serde_json::to_value(det).unwrap())
    })
}

// ---------------------------------------------------------------- renaming

pub fn plan(labels: &Value, check_disk: bool) -> Result<Value, String> {
    let det = labels_from(labels)?;
    Ok(serde_json::to_value(rename::plan_with(&det, check_disk)).unwrap())
}

pub fn start_rename(jobs: &Jobs, labels: Value) -> bool {
    jobs.start("rename", move |progress, _cancel, _child| {
        let mut det = labels_from(&labels)?;
        progress(0, 0, "Checking the new names against the drive…");
        let p = rename::plan(&det);
        if p.blocking() > 0 {
            return Err(format!("{} problem{} to fix first; nothing was renamed.", p.blocking(), if p.blocking() == 1 { "" } else { "s" }));
        }
        let stamp = now_ms().to_string();
        let out = rename::execute(&p, &stamp, &|d, t| progress(d, t, "Renaming"))?;
        rename::apply(&mut det, &p);
        Ok(json!({"labels": det, "renamed": out.renamed, "failed": out.failed, "stamp": stamp}))
    })
}

pub fn start_undo(jobs: &Jobs, labels: Value, stamp: String) -> bool {
    jobs.start("undo", move |progress, _cancel, _child| {
        let mut det = labels_from(&labels)?;
        let folders: Vec<String> = det.groups.iter().map(|g| g.folder.clone()).collect();
        let (mut restored, mut failed) = (0, vec![]);
        for (i, f) in folders.iter().enumerate() {
            progress(i, folders.len(), "Putting the old names back");
            let (out, pairs) = rename::undo_pairs(Path::new(f), &stamp);
            restored += out.renamed;
            failed.extend(out.failed);
            rename::unapply(&mut det, f, &pairs);
        }
        Ok(json!({"labels": det, "restored": restored, "failed": failed}))
    })
}

// ---------------------------------------------------------------- unpacking CZIs with Fiji

const UNPACKER: &str = include_str!("../resources/FluoraUnpack.java");

/// Fiji's folder (the one with jars/ and plugins/) and the Java inside it.
pub fn fiji_java(fiji: &str) -> Option<(PathBuf, PathBuf)> {
    let app = PathBuf::from(fiji);
    let mut roots = vec![app.clone()];
    if let Some(p) = app.parent() {
        roots.push(p.to_path_buf());
    }
    roots.push(app.join("Contents"));
    let root = roots.into_iter().find(|r| r.join("jars").is_dir())?;
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    let pattern = format!("{}/java/**/bin/{exe}", glob::Pattern::escape(&root.to_string_lossy()));
    let java = glob::glob(&pattern).ok()?.filter_map(|x| x.ok()).find(|p| p.is_file())
        .or_else(|| which_java(exe))?;
    Some((root, java))
}

fn which_java(exe: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(exe)).find(|p| p.is_file()))
}

/// Saves every series of each CZI as its own TIFF beside it (name_series0.tif, …), exactly like
/// the lab's Fiji "Unpack" macro, using Fiji's own Bio-Formats in the background.
///
/// `dest` = (the folder the CZIs were found in, a folder to save into instead), for when the
/// CZIs' own folder is read-only: TIFFs then go to the same subfolders under `dest`'s folder.
pub fn start_unpack(jobs: &Jobs, data_root: PathBuf, fiji: String, czis: Vec<String>, dest: Option<(String, String)>) -> bool {
    jobs.start("unpack", move |progress, cancel, child| {
        let (root, java) = fiji_java(&fiji).ok_or("Fiji wasn't found. Choose where Fiji is in Settings.")?;
        let src = data_root.join("FluoraUnpack.java");
        std::fs::write(&src, UNPACKER).map_err(|e| e.to_string())?;
        let sep = if cfg!(windows) { ";" } else { ":" };
        let cp = format!("{}{sep}{}", root.join("jars").join("*").display(), root.join("plugins").join("*").display());
        let mut done_files = vec![];
        for (i, czi) in czis.iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let name = Path::new(czi).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let here = Path::new(czi).parent().ok_or("Bad path")?;
            let out: PathBuf = match &dest {
                Some((from, to)) => Path::new(to).join(here.strip_prefix(from).unwrap_or(Path::new(""))),
                None => here.to_path_buf(),
            };
            let _ = std::fs::create_dir_all(&out);
            if !crate::detect::writable(&out) {
                return Err(format!("Can't save files in {} (it's read-only for you). Choose another folder to save the unpacked TIFFs in.", out.display()));
            }
            progress(0, 0, &format!("{name} ({} of {})", i + 1, czis.len()));
            let mut cmd = Command::new(&java);
            cmd.arg("-Djava.awt.headless=true").arg("-Xmx6g").arg("-cp").arg(&cp).arg(&src).arg(czi).arg(&out)
                .stdout(Stdio::piped()).stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // no console window
            }
            let mut c = cmd.spawn().map_err(|e| format!("Couldn't start Fiji's Java: {e}"))?;
            let stdout = c.stdout.take().unwrap();
            *child.lock().unwrap() = Some(c);
            let mut series = 0usize;
            let mut trouble = String::new();
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.contains("rror") || line.contains("Exception") {
                    trouble = line.trim().chars().take(200).collect();
                }
                let mut w = line.split_whitespace();
                match (w.next(), w.next(), w.next()) {
                    (Some("FLUORA"), Some("series"), Some(n)) => series = n.parse().unwrap_or(0),
                    (Some("FLUORA"), Some("saved" | "skip"), Some(n)) => {
                        let k: usize = n.parse().unwrap_or(0) + 1;
                        progress(k, series, &format!("{name} ({} of {}): image file {k} of {series}", i + 1, czis.len()));
                    }
                    _ => {}
                }
            }
            let status = child.lock().unwrap().take().map(|mut c| c.wait());
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            match status {
                Some(Ok(s)) if s.success() => done_files.push(czi.clone()),
                _ => return Err(if trouble.is_empty() {
                    format!("Fiji couldn't unpack {name}. Try opening it in Fiji to check the file.")
                } else {
                    format!("Fiji couldn't unpack {name}: {trouble}")
                }),
            }
        }
        Ok(json!({"unpacked": done_files, "cancelled": cancel.load(Ordering::SeqCst)}))
    })
}


/// Where Fiji is: the one chosen, or the first found in the usual places.
pub fn find_fiji(chosen: &str) -> Option<String> {
    if !chosen.trim().is_empty() {
        return Some(chosen.trim().to_string());
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let mut roots: Vec<PathBuf> = vec![PathBuf::from("/Applications")];
    if let Some(h) = &home {
        roots.extend([h.join("Applications"), h.clone(), h.join("Desktop"), h.join("Downloads")]);
    }
    for var in ["ProgramFiles", "LOCALAPPDATA"] {
        if let Some(v) = std::env::var_os(var) {
            roots.push(PathBuf::from(v));
        }
    }
    let patterns: &[&str] = if cfg!(target_os = "macos") {
        &["Fiji.app", "Fiji/Fiji.app", "Fiji*.app"]
    } else {
        &["Fiji*/fiji-windows-x64.exe", "Fiji*/ImageJ-win64.exe", "*/Fiji*/fiji-windows-x64.exe", "*/Fiji*/ImageJ-win64.exe"]
    };
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
    hits.pop()
}
