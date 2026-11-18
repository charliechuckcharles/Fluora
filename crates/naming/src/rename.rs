//! Renaming files to the convention, safely: every problem is found before anything is touched,
//! each folder gets a log of what was renamed (so it can be undone at any time, even after the
//! app is closed), and one failure doesn't stop the rest.
use crate::convention::{self, Special};
use crate::detect::{Detection, Group, Kind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// The log each folder keeps of renames, newest last
pub const LOG_FILE: &str = ".fluora-renames.json";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Rename {
    pub group: usize,
    /// The entry in that group, or None for the folder's CZI
    pub entry: Option<usize>,
    pub folder: String,
    pub from: String,
    pub to: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Problem {
    pub group: usize,
    pub entry: Option<usize>,
    /// "missing", "duplicate", "exists" or "note"
    pub kind: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Plan {
    /// Only files whose name changes
    pub renames: Vec<Rename>,
    /// The new name of every part, per group and entry (same order as the parts)
    pub names: Vec<Vec<Vec<String>>>,
    /// The new CZI name per group, when there is exactly one CZI
    pub czi_names: Vec<Option<String>>,
    pub problems: Vec<Problem>,
}

impl Plan {
    pub fn blocking(&self) -> usize {
        self.problems.iter().filter(|p| p.kind != "note").count()
    }
}

const LABELS: [(&str, &str); 7] = [("cond", "condition"), ("slide", "slide"), ("im", "image #"), ("stains", "stains"),
    ("obj", "objective"), ("date", "date"), ("initials", "initials")];

fn label(k: &str) -> &str {
    LABELS.iter().find(|(a, _)| *a == k).map(|(_, b)| *b).unwrap_or(k)
}

fn ext_of(name: &str) -> String {
    let l = name.to_lowercase();
    if l.ends_with(".ome.tif") || l.ends_with(".ome.tiff") {
        return l[l.find(".ome.").unwrap() + 1..].to_string();
    }
    l.rsplit('.').next().unwrap_or("tif").to_string()
}

/// Works out every new name and anything that would go wrong. Touches nothing.
pub fn plan(det: &Detection) -> Plan {
    plan_with(det, true)
}

/// Like plan; `check_disk` false skips looking for clashes with files that aren't part of the
/// set (quick enough to run on every keystroke over the VPN).
pub fn plan_with(det: &Detection, check_disk: bool) -> Plan {
    let mut p = Plan::default();
    for (gi, g) in det.groups.iter().enumerate() {
        if g.skip || g.read_only {
            p.names.push(g.entries.iter().map(|e| e.parts.iter().map(|x| x.file.name.clone()).collect()).collect());
            p.czi_names.push(None);
            if g.read_only && !g.skip {
                p.problems.push(Problem { group: gi, entry: None, kind: "note".into(),
                    message: "This folder is read-only for you, so its files keep their names".into() });
            }
            continue;
        }
        let mut gnames = vec![];
        for (ei, e) in g.entries.iter().enumerate() {
            let f = g.fields_of(e);
            let special = match e.kind {
                Kind::Label => Special::Label,
                Kind::Overview => Special::Overview,
                _ => Special::None,
            };
            let mut names = vec![];
            if e.kind == Kind::Skip {
                gnames.push(e.parts.iter().map(|x| x.file.name.clone()).collect());
                continue;
            }
            let missing = f.missing(special);
            if !missing.is_empty() {
                p.problems.push(Problem { group: gi, entry: Some(ei), kind: "missing".into(),
                    message: format!("Needs {}", missing.iter().map(|k| label(k)).collect::<Vec<_>>().join(", ")) });
            }
            for part in &e.parts {
                let ext = ext_of(&part.file.name);
                let n = match special {
                    Special::None => {
                        let g = if e.split || e.parts.len() > 1 { Some(part.tag.as_str()) } else { None };
                        convention::image_name(&f, g, &ext)
                    }
                    s => convention::special_name(&f, s, &ext),
                };
                if missing.is_empty() && n != part.file.name {
                    p.renames.push(Rename { group: gi, entry: Some(ei), folder: g.folder.clone(), from: part.file.name.clone(), to: n.clone() });
                }
                names.push(if missing.is_empty() { n } else { part.file.name.clone() });
            }
            gnames.push(names);
        }
        p.names.push(gnames);
        // The CZI takes the shared values (with the first image's, where they differ)
        let czi = if g.czis.len() == 1 {
            let first = g.entries.iter().find(|e| e.kind == Kind::Image);
            let f = first.map(|e| g.fields_of(e)).unwrap_or_else(|| g.shared.clone());
            if f.missing(Special::Czi).is_empty() {
                let n = convention::czi_name(&f);
                if n != g.czis[0].name {
                    p.renames.push(Rename { group: gi, entry: None, folder: g.folder.clone(), from: g.czis[0].name.clone(), to: n.clone() });
                }
                Some(n)
            } else {
                None
            }
        } else {
            if g.czis.len() > 1 {
                p.problems.push(Problem { group: gi, entry: None, kind: "note".into(),
                    message: format!("{} CZI files here, so they keep their names", g.czis.len()) });
            }
            None
        };
        p.czi_names.push(czi);
    }
    // Two files can't get the same name, and no rename may land on a file that's staying put
    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in &p.renames {
        *seen.entry((r.folder.clone(), r.to.to_lowercase())).or_default() += 1;
    }
    let moving: BTreeSet<(String, String)> = p.renames.iter().map(|r| (r.folder.clone(), r.from.to_lowercase())).collect();
    let mut flagged = BTreeSet::new();
    for r in &p.renames {
        let key = (r.folder.clone(), r.to.to_lowercase());
        if seen[&key] > 1 && flagged.insert((r.group, r.entry)) {
            p.problems.push(Problem { group: r.group, entry: r.entry, kind: "duplicate".into(),
                message: format!("Same new name as another image ({}). Check the image numbers.", r.to) });
        } else if check_disk && !moving.contains(&key) && Path::new(&r.folder).join(&r.to).exists() && flagged.insert((r.group, r.entry)) {
            p.problems.push(Problem { group: r.group, entry: r.entry, kind: "exists".into(),
                message: format!("A different file is already called {}", r.to) });
        }
    }
    p
}

fn read_log(folder: &Path) -> Vec<Value> {
    fs::read(folder.join(LOG_FILE)).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.as_array().cloned()).unwrap_or_default()
}

fn write_log(folder: &Path, log: &[Value]) -> std::io::Result<()> {
    let tmp = folder.join(format!("{LOG_FILE}.tmp"));
    fs::write(&tmp, serde_json::to_vec_pretty(&json!(log))?)?;
    fs::rename(&tmp, folder.join(LOG_FILE))
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub renamed: usize,
    pub failed: Vec<String>,
}

/// Renames per the plan (only if it has no blocking problems). Files are renamed in two steps
/// (to a temporary name, then the final one), so names that swap places can't collide.
/// `stamp` identifies this batch in each folder's log.
pub fn execute(plan: &Plan, stamp: &str, progress: &dyn Fn(usize, usize)) -> Result<Outcome, String> {
    if plan.blocking() > 0 {
        return Err("Fix the problems shown first; nothing was renamed.".into());
    }
    let mut by_folder: BTreeMap<String, Vec<&Rename>> = BTreeMap::new();
    for r in &plan.renames {
        by_folder.entry(r.folder.clone()).or_default().push(r);
    }
    let mut out = Outcome::default();
    let total = plan.renames.len();
    for (folder, list) in by_folder {
        let dir = Path::new(&folder);
        // Write the log before touching anything, so an undo is always possible
        let mut log = read_log(dir);
        let pairs: Vec<Value> = list.iter().map(|r| json!({"from": r.from, "to": r.to})).collect();
        log.push(json!({"batch": stamp, "renames": pairs}));
        if let Err(e) = write_log(dir, &log) {
            out.failed.push(format!("{folder}: can't write the undo log ({e}), so nothing here was renamed"));
            continue;
        }
        let tmp = |r: &Rename| format!("{}.fluora-renaming", r.from);
        let mut staged = vec![];
        for r in &list {
            match fs::rename(dir.join(&r.from), dir.join(tmp(r))) {
                Ok(()) => staged.push(*r),
                Err(e) => out.failed.push(format!("{}: {e}", r.from)),
            }
        }
        for r in staged {
            match fs::rename(dir.join(tmp(r)), dir.join(&r.to)) {
                Ok(()) => {
                    out.renamed += 1;
                    progress(out.renamed, total);
                }
                Err(e) => {
                    let _ = fs::rename(dir.join(tmp(r)), dir.join(&r.from));
                    out.failed.push(format!("{}: {e}", r.from));
                }
            }
        }
    }
    Ok(out)
}

/// The rename batches a folder's log holds (newest last), as (batch stamp, number of files).
pub fn history(folder: &Path) -> Vec<(String, usize)> {
    read_log(folder).iter().filter(|b| !b["undone"].as_bool().unwrap_or(false))
        .map(|b| (b["batch"].as_str().unwrap_or("").to_string(), b["renames"].as_array().map(|a| a.len()).unwrap_or(0))).collect()
}

/// Puts back the original names of one batch in one folder. Also returns the (from, to) pairs
/// that were put back.
pub fn undo(folder: &Path, stamp: &str) -> Outcome {
    undo_pairs(folder, stamp).0
}

pub fn undo_pairs(folder: &Path, stamp: &str) -> (Outcome, Vec<(String, String)>) {
    let mut out = Outcome::default();
    let mut pairs = vec![];
    let mut log = read_log(folder);
    for b in log.iter_mut().filter(|b| b["batch"] == stamp && !b["undone"].as_bool().unwrap_or(false)) {
        for r in b["renames"].as_array().cloned().unwrap_or_default() {
            let (from, to) = (r["from"].as_str().unwrap_or(""), r["to"].as_str().unwrap_or(""));
            if from.is_empty() || to.is_empty() {
                continue;
            }
            if folder.join(from).exists() {
                continue; // already back
            }
            match fs::rename(folder.join(to), folder.join(from)) {
                Ok(()) => {
                    out.renamed += 1;
                    pairs.push((from.to_string(), to.to_string()));
                }
                Err(e) => out.failed.push(format!("{to}: {e}")),
            }
        }
        if out.failed.is_empty() {
            b["undone"] = json!(true);
        }
    }
    let _ = write_log(folder, &log);
    (out, pairs)
}

/// After an undo, points the detection back at the old names (keeping everything typed in).
pub fn unapply(det: &mut Detection, folder: &str, pairs: &[(String, String)]) {
    for g in det.groups.iter_mut().filter(|g| g.folder == folder) {
        for e in g.entries.iter_mut() {
            for p in e.parts.iter_mut() {
                if let Some((from, _)) = pairs.iter().find(|(_, to)| *to == p.file.name) {
                    p.file.name = from.clone();
                }
            }
            e.named = e.parts.iter().all(|p| convention::parse(&p.file.name).is_some());
        }
        for c in g.czis.iter_mut() {
            if let Some((from, _)) = pairs.iter().find(|(_, to)| *to == c.name) {
                c.name = from.clone();
            }
        }
    }
}

/// After renaming, points the detection at the new names.
pub fn apply(det: &mut Detection, plan: &Plan) {
    for (gi, g) in det.groups.iter_mut().enumerate() {
        for (ei, e) in g.entries.iter_mut().enumerate() {
            for r in plan.renames.iter().filter(|r| r.group == gi && r.entry == Some(ei)) {
                if let Some(part) = e.parts.iter_mut().find(|p| p.file.name == r.from) {
                    if Path::new(&g.folder).join(&r.to).exists() {
                        part.file.name = r.to.clone();
                    }
                }
            }
            if e.kind != Kind::Skip && e.parts.iter().all(|p| convention::parse(&p.file.name).is_some()) {
                e.named = true;
            }
        }
        for r in plan.renames.iter().filter(|r| r.group == gi && r.entry.is_none()) {
            if let Some(c) = g.czis.iter_mut().find(|c| c.name == r.from) {
                if Path::new(&g.folder).join(&r.to).exists() {
                    c.name = r.to.clone();
                }
            }
        }
    }
}

/// Group helper for callers that only need one group's plan
pub fn plan_group(g: &Group) -> Plan {
    plan(&Detection { root: String::new(), groups: vec![g.clone()], ..Default::default() })
}
