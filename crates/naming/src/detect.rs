//! Looks through an image folder and works out which files belong to which image.
//!
//! Every folder holding images becomes a group (usually one slide). In each group:
//! - files already named by the convention are read from their names;
//! - other TIFFs (for example the `_series0.tif, _series1.tif...` a CZI unpacks into) are put in
//!   order and joined into pyramids: a file that is about half the width and height of the one
//!   before it (same channels) is a smaller size of the same image; anything else starts a new
//!   image. So pyramids of any depth, single-size images, and mixtures all come out right;
//! - colour (RGB) pictures at the end of a CZI's series are its slide label and overview.
use crate::convention::{self, Fields, Special};
use crate::tiffinfo::{self, TiffInfo};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Folders that only hold copies, so are never scanned
pub const SKIP_FOLDERS: [&str; 2] = ["Full Res Exports", "High Resolution Snaps"];

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FileRef {
    pub name: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<TiffInfo>,
}

/// One file of an image: a size of a pyramid ("R1" = full resolution) or one stain ("DAPI").
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Part {
    pub tag: String,
    pub file: FileRef,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    #[default]
    Image,
    Label,
    Overview,
    Skip,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Entry {
    pub kind: Kind,
    /// Largest size first, or one part per stain when `split` is true
    pub parts: Vec<Part>,
    /// True when each stain is its own file (G[DAPI], G[BF]...)
    pub split: bool,
    /// This image's own values; blank ones come from the group's shared values
    pub fields: Fields,
    /// The name already followed the convention
    pub named: bool,
    /// Found since the labels were last saved
    pub new: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Group {
    pub folder: String,
    /// Folder path inside the chosen folder, for showing
    pub rel: String,
    /// Values shared by every image in the folder
    pub shared: Fields,
    /// Shared values Fluora guessed (rather than read from names), to check
    pub guessed: Vec<String>,
    pub entries: Vec<Entry>,
    pub czis: Vec<FileRef>,
    /// Channel names found in the files, as a hint for the stains
    pub filters: Vec<String>,
    /// µm per pixel of the full-size images, as a hint for the objective
    pub um: Option<f64>,
    /// Leave this folder out (of the project and of renaming)
    pub skip: bool,
    /// Found since the labels were last saved
    pub new: bool,
    /// Files can't be saved or renamed in this folder (e.g. a protected Raw_data folder)
    pub read_only: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Detection {
    pub root: String,
    pub groups: Vec<Group>,
    /// CZI files with no TIFFs next to them yet (candidates for unpacking)
    pub packed: Vec<String>,
    /// Folders of `packed` CZIs where nothing can be saved, so unpacking needs another folder
    pub packed_read_only: Vec<String>,
}

/// True when files can be made in `dir` (tested by making and removing an empty file).
pub fn writable(dir: &Path) -> bool {
    let probe = dir.join(".fluora-write-test");
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

impl Group {
    /// The values an entry ends up with
    pub fn fields_of(&self, e: &Entry) -> Fields {
        e.fields.over(&self.shared)
    }
}

fn natural_key(s: &str) -> Vec<(String, u64)> {
    let re = Regex::new(r"(\D*)(\d*)").unwrap();
    re.captures_iter(s).map(|c| (c[1].to_lowercase(), c[2].parse().unwrap_or(0))).collect()
}

fn series_number(name: &str) -> Option<u64> {
    Regex::new(r"(?i)series[ _-]?(\d+)").unwrap().captures(name).and_then(|c| c[1].parse().ok())
}

fn is_tiff(name: &str) -> bool {
    let l = name.to_lowercase();
    (l.ends_with(".tif") || l.ends_with(".tiff")) && !l.contains(".fluora-part")
}

/// True when `b` looks like the next smaller size of `a`.
fn smaller_size_of(a: &TiffInfo, b: &TiffInfo) -> bool {
    if a.channels != b.channels || a.rgb != b.rgb || b.w >= a.w || b.h >= a.h || b.w == 0 || b.h == 0 {
        return false;
    }
    let (rx, ry) = (a.w as f64 / b.w as f64, a.h as f64 / b.h as f64);
    (1.5..=4.5).contains(&rx) && (rx - ry).abs() / rx < 0.05
}

/// The date a file was last changed, as YYYYMMDD (a good guess at when it was imaged).
fn file_date(path: &Path) -> Option<String> {
    let secs = fs::metadata(path).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    // days since 1970 → civil date (Howard Hinnant's algorithm)
    let z = secs / 86400 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    Some(format!("{y:04}{m:02}{d:02}"))
}

/// Searches `root` and every folder under it. `progress` gets a short message as it goes.
pub fn detect(root: &Path, initials: &str, progress: &dyn Fn(&str)) -> Detection {
    let mut det = Detection { root: root.to_string_lossy().into_owned(), ..Default::default() };
    walk(root, root, initials, progress, &mut det);
    det
}

fn walk(root: &Path, folder: &Path, initials: &str, progress: &dyn Fn(&str), det: &mut Detection) {
    let Ok(rd) = fs::read_dir(folder) else { return };
    let mut files: Vec<(String, u64)> = vec![];
    let mut dirs: Vec<String> = vec![];
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        if md.is_dir() {
            if !SKIP_FOLDERS.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                dirs.push(name);
            }
        } else {
            files.push((name, md.len()));
        }
    }
    files.sort_by_key(|f| natural_key(&f.0));
    dirs.sort_by_key(|d| natural_key(d));
    let rel = folder.strip_prefix(root).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    progress(&format!("Looking in {}", if rel.is_empty() { "the chosen folder" } else { &rel }));
    let czis: Vec<FileRef> = files.iter().filter(|f| f.0.to_lowercase().ends_with(".czi"))
        .map(|f| FileRef { name: f.0.clone(), size: f.1, info: None }).collect();
    let tifs: Vec<&(String, u64)> = files.iter().filter(|f| is_tiff(&f.0)).collect();
    if tifs.is_empty() {
        for c in &czis {
            det.packed.push(folder.join(&c.name).to_string_lossy().into_owned());
        }
        if !czis.is_empty() && !writable(folder) {
            det.packed_read_only.push(folder.to_string_lossy().into_owned());
        }
    } else {
        let mut g = group_for(folder, rel, &tifs, czis, initials);
        g.read_only = !writable(folder);
        det.groups.push(g);
    }
    for d in dirs {
        walk(root, &folder.join(d), initials, progress, det);
    }
}

fn group_for(folder: &Path, rel: String, tifs: &[&(String, u64)], czis: Vec<FileRef>, initials: &str) -> Group {
    let mut g = Group { folder: folder.to_string_lossy().into_owned(), rel, czis, ..Default::default() };
    // 1. Names that already follow the convention
    let mut named: BTreeMap<String, Entry> = BTreeMap::new();
    let mut order: Vec<String> = vec![];
    let mut others: Vec<&(String, u64)> = vec![];
    for f in tifs {
        let Some(p) = convention::parse(&f.0) else {
            others.push(f);
            continue;
        };
        let file = FileRef { name: f.0.clone(), size: f.1, info: None };
        let kind = match p.special {
            Special::Label => Kind::Label,
            Special::Overview => Kind::Overview,
            _ => Kind::Image,
        };
        // files of one image share everything but G[...]
        let key = format!("{:?}|{}", kind, convention::image_name(&p.fields, None, ""));
        let e = named.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Entry { kind, fields: p.fields.clone(), named: true, ..Default::default() }
        });
        let tag = p.group.clone().unwrap_or_else(|| "R1".into());
        if !Regex::new(r"^R\d+$").unwrap().is_match(&tag) {
            e.split = true;
        }
        e.parts.push(Part { tag, file });
    }
    for k in &order {
        let mut e = named.remove(k).unwrap();
        if !e.split {
            e.parts.sort_by_key(|p| p.tag[1..].parse::<u32>().unwrap_or(99));
        }
        g.entries.push(e);
    }
    // 2. Everything else: read the headers and join sizes into pyramids
    if !others.is_empty() {
        if others.iter().all(|f| series_number(&f.0).is_some()) {
            others.sort_by_key(|f| series_number(&f.0));
        }
        // headers are read side by side: over the VPN each one is mostly waiting
        use rayon::prelude::*;
        let infos: Vec<(FileRef, Option<TiffInfo>)> = others.par_iter().map(|f| {
            let info = tiffinfo::read(&folder.join(&f.0)).ok();
            (FileRef { name: f.0.clone(), size: f.1, info: info.clone() }, info)
        }).collect();
        let mut chains: Vec<Vec<FileRef>> = vec![];
        let mut prev: Option<TiffInfo> = None;
        for (file, info) in infos {
            let joins = matches!((&prev, &info), (Some(a), Some(b)) if smaller_size_of(a, b));
            prev = info;
            if joins {
                chains.last_mut().unwrap().push(file);
            } else {
                chains.push(vec![file]);
            }
        }
        // Colour pictures at the very end of a CZI's series are its label and overview
        let main_rgb = chains.iter().find(|c| c[0].info.as_ref().is_some_and(|i| !i.rgb)).is_none();
        let mut specials: Vec<usize> = vec![];
        if !main_rgb {
            for i in (0..chains.len()).rev().take(2) {
                let c = &chains[i];
                if c.len() == 1 && c[0].info.as_ref().is_some_and(|x| x.rgb) {
                    specials.insert(0, i);
                } else {
                    break;
                }
            }
        }
        let first_im = g.entries.iter().filter(|e| e.kind == Kind::Image).count();
        let mut im = first_im;
        for (i, c) in chains.into_iter().enumerate() {
            let kind = match specials.iter().position(|s| *s == i) {
                Some(j) if specials.len() == 2 && j == 0 => Kind::Label,
                Some(_) => Kind::Overview,
                None => Kind::Image,
            };
            let mut fields = Fields::default();
            if kind == Kind::Image {
                im += 1;
                fields.im = format!("{im:02}");
            }
            let parts = c.into_iter().enumerate().map(|(j, file)| Part { tag: format!("R{}", j + 1), file }).collect();
            g.entries.push(Entry { kind, parts, split: false, fields, named: false, new: false });
        }
    }
    read_czi_details(&mut g, folder);
    // Unnamed images: start their stains from the channel names in the file (on a confocal these
    // are usually the dyes, like DAPI and AF633; on the slide scanner they're filters, so check)
    let mut stains_guessed = false;
    for e in g.entries.iter_mut().filter(|e| !e.named && e.kind == Kind::Image && e.fields.stains.is_empty()) {
        let names: Vec<String> = e.parts.first().and_then(|p| p.file.info.as_ref()).map(|i| i.filters.clone()).unwrap_or_default();
        let mut list: Vec<String> = vec![];
        // the slide scanner names its green filter set EGFP; the stain is usually GFP
        for n in names.iter().map(|n| if n.eq_ignore_ascii_case("EGFP") { "GFP".to_string() } else { convention::clean(n) }).filter(|n| !n.is_empty() && !Regex::new(r"^(?i)ch\d+$").unwrap().is_match(n)) {
            if !list.contains(&n) {
                list.push(n);
            }
        }
        if !list.is_empty() {
            e.fields.stains = list.join("-");
            stains_guessed = true;
        }
    }
    share_and_guess(&mut g, folder, initials);
    if stains_guessed && !g.shared.stains.is_empty() && !g.guessed.iter().any(|x| x == "stains") {
        g.guessed.push("stains".into());
    }
    g
}

/// The date taken and the objective, for images unpacked from a CZI: read from that CZI (the
/// one the unpacker noted, or the one whose name starts the file's name, or the folder's only one).
fn read_czi_details(g: &mut Group, folder: &Path) {
    let czis = g.czis.clone();
    let mut seen: BTreeMap<std::path::PathBuf, Option<crate::czi::CziMeta>> = BTreeMap::new();
    for e in g.entries.iter_mut().filter(|e| !e.named && e.kind == Kind::Image) {
        let Some(first) = e.parts.first().map(|p| p.file.clone()) else { continue };
        let czi = first.info.as_ref().and_then(|i| i.source.clone()).map(std::path::PathBuf::from).filter(|p| p.exists())
            .or_else(|| czis.iter().find(|c| first.name.starts_with(&format!("{}_series", convention::stem(&c.name)))).map(|c| folder.join(&c.name)))
            .or_else(|| (czis.len() == 1).then(|| folder.join(&czis[0].name)));
        let Some(path) = czi else { continue };
        let meta = seen.entry(path.clone()).or_insert_with(|| crate::czi::read_meta(&path)).clone();
        if let Some(m) = meta {
            if let Some(d) = m.date {
                e.fields.date = d;
            }
            if let Some(o) = m.objective {
                e.fields.obj = o;
            }
        }
    }
}

/// Moves values every image has in common to the group's shared values, and fills in sensible
/// guesses for unnamed images (slide number from the folder or CZI name, date from the CZI).
fn share_and_guess(g: &mut Group, folder: &Path, initials: &str) {
    let images: Vec<usize> = (0..g.entries.len()).filter(|i| g.entries[*i].kind == Kind::Image).collect();
    // a value every image has (read from names or CZIs) goes in the shared row
    for k in convention::ALL {
        if k == "im" || images.is_empty() {
            continue;
        }
        let vals: Vec<&str> = images.iter().map(|i| g.entries[*i].fields.get(k)).collect();
        if !vals[0].is_empty() && vals.iter().all(|v| *v == vals[0]) {
            let v = vals[0].to_string();
            g.shared.set(k, v.clone());
            for i in g.entries.iter_mut() {
                if i.fields.get(k) == v {
                    i.fields.set(k, String::new());
                }
            }
        }
    }
    let unnamed = images.iter().any(|i| !g.entries[*i].named);
    if unnamed {
        // A CZI already named by the convention says it all
        if let Some(p) = g.czis.iter().find_map(|c| convention::parse(&c.name)) {
            for k in convention::ALL {
                if g.shared.get(k).is_empty() && !p.fields.get(k).is_empty() && k != "im" {
                    g.shared.set(k, p.fields.get(k).to_string());
                }
            }
        }
        let folder_name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if g.shared.slide.is_empty() {
            let slide = Regex::new(r"(?i)(?:^|[^a-z])(?:SL|slide)[ _-]?(\d+)").unwrap().captures(&folder_name).map(|c| c[1].to_string())
                .or_else(|| folder_name.chars().all(|c| c.is_ascii_digit()).then(|| folder_name.clone()).filter(|s| !s.is_empty()));
            if let Some(s) = slide {
                g.shared.slide = s;
                g.guessed.push("slide".into());
            }
        }
        if g.shared.date.is_empty() && g.entries.iter().all(|e| e.fields.date.is_empty()) {
            let src = g.czis.first().map(|c| folder.join(&c.name))
                .or_else(|| g.entries.iter().find(|e| !e.named).and_then(|e| e.parts.first()).map(|p| folder.join(&p.file.name)));
            if let Some(d) = src.and_then(|p| file_date(&p)) {
                g.shared.date = d;
                g.guessed.push("date".into());
            }
        }
        if g.shared.initials.is_empty() && !initials.is_empty() {
            g.shared.initials = initials.to_string();
            g.guessed.push("initials".into());
        }
    }
    // Hints: channel names and pixel size of the first full-size image we have headers for
    let first = g.entries.iter().filter(|e| e.kind == Kind::Image).find_map(|e| e.parts.first()?.file.info.clone())
        .or_else(|| {
            let e = g.entries.iter().find(|e| e.kind == Kind::Image)?;
            if e.split {
                return None;
            }
            let smallest = e.parts.last()?;
            // (the slide scanner's exports keep the full-size pixel size in every size's file)
            tiffinfo::read(&folder.join(&smallest.file.name)).ok()
        });
    if let Some(i) = first {
        g.filters = i.filters.iter().filter(|f| !f.is_empty()).cloned().collect();
        g.um = i.um;
    }
    if g.shared.obj.is_empty() && g.entries.iter().all(|e| e.fields.obj.is_empty()) {
        if let Some(o) = objective_hint(g.um) {
            g.shared.obj = o;
            g.guessed.push("obj".into());
        }
    }
    if g.entries.iter().any(|e| e.split) {
        let mut tags: Vec<String> = vec![];
        for e in &g.entries {
            for p in e.parts.iter().filter(|_| e.split) {
                if !tags.contains(&p.tag) {
                    tags.push(p.tag.clone());
                }
            }
        }
        g.filters = tags;
    }
}

/// Guesses the objective from the pixel size (Zeiss Axioscan-style cameras).
pub fn objective_hint(um: Option<f64>) -> Option<String> {
    let um = um?;
    [(0.65, "10x"), (0.325, "20x"), (0.1625, "40x"), (0.11, "63x")].iter()
        .find(|(u, _)| (um - u).abs() / u < 0.12).map(|(_, o)| o.to_string())
}

/// Brings saved labels up to date with a fresh look at the folder: everything typed in before
/// is kept, images and folders that are new are added (marked `new`), and files that are gone
/// are dropped.
pub fn merge(saved: &Detection, fresh: &Detection) -> Detection {
    let mut out = Detection { root: fresh.root.clone(), packed: fresh.packed.clone(), packed_read_only: fresh.packed_read_only.clone(), groups: vec![] };
    for fg in &fresh.groups {
        let Some(sg) = saved.groups.iter().find(|g| g.folder == fg.folder) else {
            let mut g = fg.clone();
            g.new = true;
            g.entries.iter_mut().for_each(|e| e.new = true);
            out.groups.push(g);
            continue;
        };
        let mut g = sg.clone();
        g.new = false;
        g.read_only = fg.read_only;
        g.czis = fg.czis.clone();
        g.entries = vec![];
        let next_im = sg.entries.iter().filter_map(|e| e.fields.im.parse::<u32>().ok()).max().unwrap_or(0);
        let mut added = 0;
        for fe in &fg.entries {
            let first = fe.parts.first().map(|p| p.file.name.as_str()).unwrap_or("");
            if let Some(se) = sg.entries.iter().find(|e| e.parts.iter().any(|p| p.file.name == first)) {
                let mut e = se.clone();
                e.new = false;
                e.parts = fe.parts.clone();
                e.split = fe.split;
                g.entries.push(e);
            } else {
                let mut e = fe.clone();
                e.new = true;
                if !e.named && e.kind == Kind::Image {
                    added += 1;
                    e.fields.im = format!("{:02}", next_im + added);
                }
                g.entries.push(e);
            }
        }
        out.groups.push(g);
    }
    out
}
