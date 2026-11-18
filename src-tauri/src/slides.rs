// Finds the slices in an image folder and makes the quick-look images.
//
// A project's slices come from its labels (labels.json, made in the setup steps), or, for a
// project made before labels existed, straight from file names like
//     01q_E_SL04_IM01_DAPI-GAF_G[R1]_WB_20250730_CNH.tif
// Each slice gets a preview-size TIFF in the project's cache/ (the image folder is only read):
// the R4 size copied as it is when there is one, otherwise one Fluora makes by shrinking the
// image as it reads it (single-size images, per-stain files, compressed files). One
// brightness/contrast setting is worked out per stain so slides are comparable by eye, and
// JPEGs are written to images/:
//     <id>_merged.jpg   colour merge at preview size
//     <id>_thumb.jpg    colour merge, 360 px wide, for the grid
//     <id>_<STAIN>.jpg  each channel in greyscale at preview size
use crate::store::{now, read_json, save_json};
use crate::tiffread::{self, Channels};
use fluora_naming::{tiffinfo, Detection, Kind};
use image::codecs::jpeg::JpegEncoder;
use image::ExtendedColorType;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub const EXPORT_FOLDER: &str = "Full Res Exports"; // written by the full-res export; skipped when scanning
const THUMB_W: u32 = 360;
/// Longest side of a preview Fluora makes itself
const PREVIEW_MAX: u32 = 4000;
/// A ready-made size is used as the preview when its longest side is at most this
const PREVIEW_COPY_MAX: u32 = 6000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub filter: String,
    pub stain: String,
    /// "#rrggbb"
    #[serde(default)]
    pub color: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Slice {
    pub id: String,
    pub slide: u32,
    pub im: u32,
    #[serde(default)]
    pub block: String,
    #[serde(default)]
    pub cond: String,
    pub stains: Vec<String>,
    pub folder: String,
    /// The sizes of the image: 1 = full resolution
    pub levels: BTreeMap<u32, String>,
    /// For images saved one stain per file: stain → file
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub split: BTreeMap<String, String>,
    #[serde(default)]
    pub czi: Option<String>,
    /// The size copied as the preview, or 0 when Fluora made the preview itself
    #[serde(default)]
    pub preview_level: u32,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub preview_size: [u32; 2],
    /// Full-resolution width and height
    #[serde(default)]
    pub full_size: [u32; 2],
    /// Width and height of each size, where known
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sizes: BTreeMap<u32, [u32; 2]>,
    /// µm per full-resolution pixel
    #[serde(default)]
    pub um_per_px: Option<f64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub obj: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub focus: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub initials: String,
}

impl Slice {
    pub fn cache_path(&self, dir: &Path) -> PathBuf {
        let tag = if self.preview_level == 0 { "P".to_string() } else { format!("R{}", self.preview_level) };
        dir.join("cache").join(format!("{}_{tag}.tif", self.id))
    }
    /// The full-resolution file (R1), or the biggest size there is.
    pub fn full_res(&self) -> Option<&String> {
        self.levels.get(&1).or_else(|| self.levels.iter().next().map(|(_, v)| v)).or_else(|| self.split.values().next())
    }
    /// Width and height of a size (1 = full resolution)
    pub fn size_of(&self, level: u32) -> [u32; 2] {
        if let Some(s) = self.sizes.get(&level) {
            return *s;
        }
        let f = 2u32.pow(level.saturating_sub(1));
        [self.full_size[0].div_ceil(f), self.full_size[1].div_ceil(f)]
    }
}

// ---------------------------------------------------------------- stains and colours

/// Colour of each stain in the merged images. The colour dots in the app use this same list, so
/// they always match the images. Stains not listed fall back to their Zeiss filter's colour.
const STAIN_COLOR: &[(&str, [u8; 3])] = &[
    ("DAPI", [0, 110, 255]), ("CALCOFLUOR", [255, 0, 255]), ("GAF", [0, 255, 0]),
    ("SYTOX", [0, 255, 0]), ("PHALLOIDIN", [255, 215, 0]), ("PI", [255, 0, 0]),
    ("HOECHST", [0, 110, 255]), ("BF", [200, 200, 200]), ("GFP", [0, 255, 0]),
    ("Red", [255, 0, 0]), ("Green", [0, 255, 0]), ("Blue", [0, 0, 255]),
];
const FILTER_COLOR: &[(&str, [u8; 3])] = &[
    ("DAPI", [0, 161, 255]), ("EGFP", [0, 255, 0]), ("DsRed", [255, 40, 0]), ("mPlum", [255, 0, 255]),
];

pub fn color_for(stain: &str, filter: &str) -> [u8; 3] {
    let find = |t: &[(&str, [u8; 3])], k: &str| t.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, c)| *c);
    find(STAIN_COLOR, stain).or_else(|| find(FILTER_COLOR, filter)).unwrap_or([255, 255, 255])
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Colours for the app's dots: the built-in list, then this project's own choices.
pub fn stain_colors_hex(project: &Path) -> Value {
    let mut m = Map::new();
    for (n, c) in FILTER_COLOR.iter().chain(STAIN_COLOR.iter()) {
        m.insert(n.to_string(), json!(hex(*c)));
    }
    if let Some(o) = read_json(&project.join("channels.json"), json!({})).as_object() {
        for v in o.values() {
            if let (Some(st), Some(c)) = (v["stain"].as_str(), v["color"].as_str()) {
                m.insert(st.to_string(), json!(c));
            }
        }
    }
    Value::Object(m)
}

/// Fluora's first guess at which stain a channel shows, from the filter name and the stains in
/// the file name.
pub fn guess_stain(filter: &str, stains: &[String]) -> String {
    let has = |x: &str| stains.iter().any(|s| s.eq_ignore_ascii_case(x));
    let f = filter;
    if f.eq_ignore_ascii_case("DsRed") && has("PHALLOIDIN") {
        return "PHALLOIDIN".into();
    }
    if f.eq_ignore_ascii_case("mPlum") && has("PI") {
        return "PI".into();
    }
    if f.eq_ignore_ascii_case("EGFP") {
        if has("SYTOX") {
            return "SYTOX".into();
        }
        if has("GAF") {
            return "GAF".into();
        }
        // the slide scanner's green filter set; what it usually shows is GFP
        return "GFP".into();
    }
    if f.eq_ignore_ascii_case("DAPI") {
        return if has("CALCOFLUOR") && !has("DAPI") { "CALCOFLUOR".into() } else { "DAPI".into() };
    }
    if has(f) {
        return f.to_uppercase();
    }
    f.to_string()
}

pub type Choices = BTreeMap<String, (String, Option<[u8; 3]>)>;

/// A project's choices for its channels: Fluora's guess → the stain name and colour to use.
pub fn channel_choices(project: &Path) -> Choices {
    let v = read_json(&project.join("channels.json"), json!({}));
    v.as_object().map(|o| o.iter().map(|(k, x)| {
        let stain = x["stain"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or(k).trim().to_string();
        (k.clone(), (stain, x["color"].as_str().and_then(parse_hex)))
    }).collect()).unwrap_or_default()
}

/// The channel list for a slice: guessed stains, then the project's choices.
pub fn channels_for(filters: &[String], s: &Slice, choices: &Choices) -> Vec<Channel> {
    filters.iter().map(|f| {
        let guess = if s.split.is_empty() { guess_stain(f, &s.stains) } else { f.clone() };
        let (stain, color) = choices.get(&guess).cloned().unwrap_or((guess.clone(), None));
        let color = color.unwrap_or_else(|| color_for(&stain, f));
        Channel { filter: f.clone(), stain, color: hex(color) }
    }).collect()
}

// ---------------------------------------------------------------- scanning the image folder

fn name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // <block>_<condition>_SL<slide>_IM<image>_<STAIN-STAIN>_G[R<level>]... ; block and condition are optional
    RE.get_or_init(|| Regex::new(r"^(?:(?P<prefix>[^/]*?)_)?SL(?P<slide>\d+)_IM(?P<im>\d+)_(?P<stains>[A-Za-z0-9-]+?)_G\[R(?P<level>\d+)\]").unwrap())
}

/// An id for a slice, made unique when the same slide and image number appear in two folders.
fn unique_id(slices: &BTreeMap<String, Slice>, base: String, folder: &str, root: &Path) -> String {
    if slices.get(&base).is_some_and(|s| s.folder != folder) {
        let rel = Path::new(folder).strip_prefix(root).unwrap_or(Path::new(folder)).to_string_lossy().into_owned();
        let clean: Vec<char> = Regex::new(r"\W+").unwrap().replace_all(&rel, "").chars().collect();
        return format!("{base}_{}", clean[clean.len().saturating_sub(12)..].iter().collect::<String>());
    }
    base
}

/// Every slice in the image folder and its subfolders, grouped from its G[R1..R6] files. Used for
/// projects made before labels existed.
pub fn scan_folder(root: &Path) -> Vec<Slice> {
    let mut slices: BTreeMap<String, Slice> = BTreeMap::new();
    walk(root, root, &mut slices);
    let mut v: Vec<Slice> = slices.into_values().collect();
    v.sort_by(|a, b| (a.slide, a.im, &a.id).cmp(&(b.slide, b.im, &b.id)));
    v
}

fn walk(root: &Path, folder: &Path, slices: &mut BTreeMap<String, Slice>) {
    let Ok(rd) = fs::read_dir(folder) else { return };
    let mut names: Vec<String> = rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.')).collect();
    names.sort();
    let (mut files, mut dirs) = (vec![], vec![]);
    for n in names {
        let p = folder.join(&n);
        if p.is_dir() {
            if n != EXPORT_FOLDER {
                dirs.push(n);
            }
        } else {
            files.push(n);
        }
    }
    let czis: Vec<&String> = files.iter().filter(|f| f.to_lowercase().ends_with(".czi")).collect();
    let folder_s = folder.to_string_lossy().into_owned();
    for f in &files {
        let lower = f.to_lowercase();
        if !(lower.ends_with(".tif") || lower.ends_with(".tiff")) {
            continue;
        }
        let Some(m) = name_re().captures(f) else { continue };
        let slide: u32 = m["slide"].parse().unwrap_or(0);
        let im_s = &m["im"];
        let prefix = m.name("prefix").map(|x| x.as_str()).unwrap_or("");
        let mut parts = prefix.split('_');
        let block = parts.next().unwrap_or("").to_string();
        let cond = parts.next().unwrap_or("").to_string();
        let sid = unique_id(slices, format!("SL{slide:02}_IM{im_s}"), &folder_s, root);
        let slide_re = Regex::new(&format!(r"_SL0*{slide}_")).unwrap();
        let czi = czis.iter().find(|c| slide_re.is_match(c)).copied()
            .or(if czis.len() == 1 { Some(czis[0]) } else { None })
            .map(|c| folder.join(c).to_string_lossy().into_owned());
        let s = slices.entry(sid.clone()).or_insert_with(|| Slice {
            id: sid.clone(), slide, im: im_s.parse().unwrap_or(0), block, cond,
            stains: m["stains"].to_uppercase().split('-').map(String::from).collect(),
            folder: folder_s.clone(), czi, ..Default::default()
        });
        s.levels.insert(m["level"].parse().unwrap_or(1), folder.join(f).to_string_lossy().into_owned());
    }
    for d in dirs {
        walk(root, &folder.join(d), slices);
    }
}

fn digits(s: &str) -> u32 {
    s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

/// The slices a project's labels describe (every image not set to skip).
pub fn slices_from_labels(det: &Detection) -> Vec<Slice> {
    let root = Path::new(&det.root);
    let mut slices: BTreeMap<String, Slice> = BTreeMap::new();
    for g in det.groups.iter().filter(|g| !g.skip) {
        let folder = Path::new(&g.folder);
        let czi = (g.czis.len() == 1).then(|| folder.join(&g.czis[0].name).to_string_lossy().into_owned());
        for e in g.entries.iter().filter(|e| e.kind == Kind::Image && !e.parts.is_empty()) {
            let f = g.fields_of(e);
            let (slide, im) = (digits(&f.slide), digits(&f.im));
            let id = unique_id(&slices, format!("SL{slide:02}_IM{im:02}"), &g.folder, root);
            let mut s = Slice {
                id: id.clone(), slide, im, block: f.sample.clone(), cond: f.cond.clone(),
                stains: f.stains.split(['-', ',', ' ']).filter(|x| !x.is_empty()).map(|x| x.to_uppercase()).collect(),
                folder: g.folder.clone(), czi: czi.clone(), obj: f.obj.clone(), focus: f.focus.clone(), date: f.date.clone(),
                initials: f.initials.clone(), ..Default::default()
            };
            for (i, p) in e.parts.iter().enumerate() {
                let path = folder.join(&p.file.name).to_string_lossy().into_owned();
                if e.split {
                    s.split.insert(p.tag.clone(), path);
                } else {
                    let level = p.tag.strip_prefix('R').and_then(|n| n.parse().ok()).unwrap_or(i as u32 + 1);
                    if let Some(info) = &p.file.info {
                        s.sizes.insert(level, [info.w, info.h]);
                        // the full-size file's own pixel size is the full-resolution one
                        if level == 1 && info.um.is_some() {
                            s.um_per_px = info.um;
                        }
                    }
                    s.levels.insert(level, path);
                }
            }
            slices.insert(id, s);
        }
    }
    let mut v: Vec<Slice> = slices.into_values().collect();
    v.sort_by(|a, b| (a.slide, a.im, &a.id).cmp(&(b.slide, b.im, &b.id)));
    v
}

/// The size copied as the preview: R4 when it exists, else the smallest above R1.
pub fn preview_level(levels: &BTreeMap<u32, String>) -> u32 {
    if levels.contains_key(&4) {
        return 4;
    }
    let above: Vec<u32> = levels.keys().copied().filter(|k| *k > 1).collect();
    if levels.len() > 1 && !above.is_empty() { *above.iter().max().unwrap() } else { *levels.keys().next().unwrap_or(&1) }
}

// ---------------------------------------------------------------- reading the TIFFs

/// All channels of a TIFF at full size.
pub fn read_channels(path: &Path) -> Result<Channels, String> {
    tiffread::read_window(path, None, 1)
}

/// The channels of a one-stain-per-file image: one plane per file (colour files become grey),
/// all shrunk by `k` and cropped to `frac`, named by their stain.
pub fn read_split(s: &Slice, frac: Option<[f64; 4]>, k: u32) -> Result<Channels, String> {
    let mut out: Option<Channels> = None;
    for (tag, path) in &s.split {
        let c = tiffread::read_window(Path::new(path), frac, k)?;
        let plane: Vec<u16> = if c.rgb {
            (0..c.data[0].len()).map(|i| ((c.data[0][i] as u32 * 299 + c.data[1][i] as u32 * 587 + c.data[2][i] as u32 * 114) / 1000) as u16).collect()
        } else {
            c.data[0].clone()
        };
        match &mut out {
            None => out = Some(Channels { data: vec![plane], w: c.w, h: c.h, filters: vec![tag.clone()], um: c.um, rgb: false }),
            Some(o) => {
                if (c.w, c.h) != (o.w, o.h) {
                    return Err(format!("{tag} is a different size from the other stains of {}", s.id));
                }
                o.data.push(plane);
                o.filters.push(tag.clone());
            }
        }
    }
    out.ok_or_else(|| "No files".to_string())
}

/// Makes sure the slice has its preview TIFF in the cache, and notes the sizes.
fn prepare_cache(s: &mut Slice, dir: &Path) -> Result<(), String> {
    // `um` = µm per full-resolution pixel, which every preview records (like the scanner's exports)
    let make = |s: &mut Slice, src: Option<&Path>, w: u32, h: u32, um: Option<f64>| -> Result<(), String> {
        let k = w.max(h).div_ceil(PREVIEW_MAX).max(1);
        s.preview_level = 0;
        s.full_size = [w, h];
        let dst = s.cache_path(dir);
        if cache_current(&dst, s) {
            return Ok(());
        }
        let mut ch = match src {
            Some(p) => tiffread::read_window(p, None, k)?,
            None => read_split(s, None, k)?,
        };
        if um.is_some() {
            ch.um = um;
        }
        tiffread::write_cache(&dst, &ch)?;
        note_source(&dst, s);
        Ok(())
    };
    if !s.split.is_empty() {
        let first = s.split.values().next().unwrap().clone();
        let i = tiffinfo::read(Path::new(&first))?;
        return make(s, None, i.w, i.h, i.um);
    }
    let lp = preview_level(&s.levels);
    let src = PathBuf::from(&s.levels[&lp]);
    let mut full_um = None;
    // The slide scanner's pyramids: copy the R4 size as it is (fast, and exactly as before)
    let ready = if s.levels.len() >= 3 && lp > 1 && !s.sizes.contains_key(&lp) {
        true
    } else {
        let i = tiffinfo::read(&src)?;
        s.sizes.insert(lp, [i.w, i.h]);
        full_um = i.um;
        if let Some(full) = s.levels.get(&1).filter(|_| lp != 1) {
            if let Ok(fi) = tiffinfo::read(Path::new(full)) {
                s.sizes.insert(1, [fi.w, fi.h]);
                full_um = fi.um;
            }
        }
        i.w.max(i.h) <= PREVIEW_COPY_MAX && lp > 1
    };
    if !ready {
        let [w, h] = s.sizes[&lp];
        let full = s.sizes.get(&1).copied().unwrap_or([w, h]);
        make(s, Some(&src), w, h, full_um)?;
        s.full_size = full;
        return Ok(());
    }
    s.preview_level = lp;
    let dst = s.cache_path(dir);
    let src_len = fs::metadata(&src).map_err(|e| format!("{}: {e}", src.display()))?.len();
    if fs::metadata(&dst).map(|m| m.len()).ok() != Some(src_len) || !cache_current(&dst, s) {
        let part = dst.with_extension("tif.part");
        fs::copy(&src, &part).map_err(|e| format!("Copying {}: {e}", s.id))?;
        fs::rename(&part, &dst).map_err(|e| e.to_string())?;
        note_source(&dst, s);
    }
    Ok(())
}

/// What a preview was made from, so a preview is remade when labels point its slice elsewhere
fn source_of(s: &Slice) -> String {
    if s.split.is_empty() {
        s.levels.get(&preview_level(&s.levels)).cloned().unwrap_or_default()
    } else {
        s.split.values().cloned().collect::<Vec<_>>().join("\n")
    }
}

fn note_source(cache: &Path, s: &Slice) {
    let _ = fs::write(cache.with_extension("tif.src"), source_of(s));
}

/// True when the cached preview exists and was made from this slice's file (previews made
/// before this was recorded are trusted).
fn cache_current(cache: &Path, s: &Slice) -> bool {
    cache.exists() && fs::read_to_string(cache.with_extension("tif.src")).map(|x| x == source_of(s)).unwrap_or(true)
}

// ---------------------------------------------------------------- rendering

fn histogram(px: &[u16]) -> Vec<u64> {
    let mut h = vec![0u64; 65536];
    for &v in px {
        h[v as usize] += 1;
    }
    h
}

fn percentile(hist: &[f64], p: f64) -> f64 {
    let total: f64 = hist.iter().sum();
    let target = total * p / 100.0;
    let mut cum = 0.0;
    for (v, n) in hist.iter().enumerate() {
        cum += n;
        if cum >= target && cum > 0.0 {
            return v as f64;
        }
    }
    65535.0
}

struct Contrast {
    min: f64,
    max: f64,
    gamma: f64,
    show: bool,
}

fn contrast_for(contrast: &Value, stain: &str, ch: &[u16]) -> Contrast {
    let c = &contrast[stain];
    if c.is_object() {
        return Contrast {
            min: c["min"].as_f64().unwrap_or(0.0),
            max: c["max"].as_f64().unwrap_or(65535.0),
            gamma: c["gamma"].as_f64().filter(|g| *g > 0.0).unwrap_or(1.0),
            show: c["show"].as_bool().unwrap_or(true),
        };
    }
    let h: Vec<f64> = histogram(ch).into_iter().map(|x| x as f64).collect();
    Contrast { min: 0.0, max: percentile(&h, 99.8), gamma: 1.0, show: true }
}

/// 0..1 brightness for every possible 16-bit value.
fn lut(c: &Contrast) -> Vec<f32> {
    let span = (c.max - c.min).max(1.0);
    (0..65536u32).map(|v| {
        let x = ((v as f64 - c.min) / span).clamp(0.0, 1.0);
        (if c.gamma != 1.0 { x.powf(1.0 / c.gamma) } else { x }) as f32
    }).collect()
}

fn save_jpeg(path: &Path, buf: &[u8], w: u32, h: u32, color: ExtendedColorType, quality: u8) -> Result<(), String> {
    let tmp = path.with_extension("jpg.part");
    let f = File::create(&tmp).map_err(|e| e.to_string())?;
    JpegEncoder::new_with_quality(BufWriter::new(f), quality).encode(buf, w, h, color).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Stain and colour of each channel read from a file: the slice's own list when it matches,
/// otherwise a fresh guess.
fn stain_colors(s: &Slice, ch: &Channels) -> Vec<(String, [f32; 3])> {
    let list = if s.channels.len() == ch.data.len() { s.channels.clone() } else { channels_for(&ch.filters, s, &BTreeMap::new()) };
    list.iter().map(|c| {
        let col = parse_hex(&c.color).unwrap_or_else(|| color_for(&c.stain, &c.filter));
        (c.stain.clone(), col.map(|x| x as f32 / 255.0))
    }).collect()
}

/// The colour picture of some channels, as RGB bytes: the merge of every shown channel, or one
/// stain in greyscale. `bright` is the viewer's screen brightness.
pub fn compose(s: &Slice, ch: &Channels, contrast: &Value, only: Option<&str>, bright: f32) -> Vec<u8> {
    let n = (ch.w * ch.h) as usize;
    let mut rgb = vec![0f32; n * 3];
    for (px, (stain, col)) in ch.data.iter().zip(stain_colors(s, ch)) {
        if only.is_some_and(|o| o != stain) {
            continue;
        }
        let c = contrast_for(contrast, &stain, px);
        if only.is_none() && !c.show {
            continue;
        }
        let table = lut(&c);
        let col = if only.is_some() { [1.0f32; 3] } else { col };
        for (i, &v) in px.iter().enumerate() {
            let x = table[v as usize] * bright;
            rgb[i * 3] += x * col[0];
            rgb[i * 3 + 1] += x * col[1];
            rgb[i * 3 + 2] += x * col[2];
        }
    }
    rgb.iter().map(|x| (x.clamp(0.0, 1.0) * 255.0) as u8).collect()
}

pub fn render(s: &Slice, ch: &Channels, contrast: &Value, img_dir: &Path) -> Result<(), String> {
    let n = (ch.w * ch.h) as usize;
    let mut rgb = vec![0f32; n * 3];
    for (px, (stain, col)) in ch.data.iter().zip(stain_colors(s, ch)) {
        let c = contrast_for(contrast, &stain, px);
        let table = lut(&c);
        let gray: Vec<u8> = px.iter().map(|&v| (table[v as usize] * 255.0) as u8).collect();
        save_jpeg(&img_dir.join(format!("{}_{stain}.jpg", s.id)), &gray, ch.w, ch.h, ExtendedColorType::L8, 85)?;
        if c.show {
            for (i, &v) in px.iter().enumerate() {
                let x = table[v as usize];
                rgb[i * 3] += x * col[0];
                rgb[i * 3 + 1] += x * col[1];
                rgb[i * 3 + 2] += x * col[2];
            }
        }
    }
    let bytes: Vec<u8> = rgb.iter().map(|x| (x.clamp(0.0, 1.0) * 255.0) as u8).collect();
    save_jpeg(&img_dir.join(format!("{}_merged.jpg", s.id)), &bytes, ch.w, ch.h, ExtendedColorType::Rgb8, 88)?;
    let im = image::RgbImage::from_raw(ch.w, ch.h, bytes).ok_or("bad image size")?;
    let k = (THUMB_W as f64 / ch.w as f64).min(THUMB_W as f64 * 2.0 / ch.h as f64).min(1.0);
    let (tw, th) = (((ch.w as f64 * k).round() as u32).max(1), ((ch.h as f64 * k).round() as u32).max(1));
    let thumb = image::imageops::resize(&im, tw, th, image::imageops::FilterType::CatmullRom);
    save_jpeg(&img_dir.join(format!("{}_thumb.jpg", s.id)), thumb.as_raw(), tw, th, ExtendedColorType::Rgb8, 82)
}

// ---------------------------------------------------------------- building a project's previews

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BuildState {
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub message: String,
    pub error: Option<String>,
    pub finished_at: Option<f64>,
    pub project: Option<String>,
}

/// Copies or makes the preview TIFFs and the JPEGs, in the background.
pub struct Builder {
    state: Arc<Mutex<BuildState>>,
    done: Arc<AtomicUsize>,
    running: Arc<AtomicBool>,
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            state: Arc::new(Mutex::new(BuildState { phase: "idle".into(), done: 0, total: 0, message: String::new(),
                error: None, finished_at: None, project: None })),
            done: Arc::new(AtomicUsize::new(0)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn status(&self) -> BuildState {
        let mut s = self.state.lock().unwrap().clone();
        s.done = self.done.load(Ordering::Relaxed);
        s
    }

    /// Starts a build for a project folder; false if one is already running.
    pub fn start(&self, project: String, dir: PathBuf, image_folder: String, rerender_only: bool) -> bool {
        if self.running.swap(true, Ordering::SeqCst) {
            return false;
        }
        let (state, done, running) = (self.state.clone(), self.done.clone(), self.running.clone());
        {
            let mut s = state.lock().unwrap();
            *s = BuildState { phase: "scanning".into(), done: 0, total: 0, message: String::new(), error: None,
                finished_at: None, project: Some(project) };
        }
        done.store(0, Ordering::Relaxed);
        std::thread::spawn(move || {
            let set = |phase: &str, total: usize, msg: &str| {
                let mut s = state.lock().unwrap();
                s.phase = phase.into();
                s.total = total;
                s.message = msg.into();
                done.store(0, Ordering::Relaxed);
            };
            let result = build(&dir, &image_folder, rerender_only, &set, &done);
            {
                let mut s = state.lock().unwrap();
                match result {
                    Ok(()) => { s.phase = "done".into(); s.message.clear(); s.finished_at = Some(now()); }
                    Err(e) => { s.phase = "error".into(); s.error = Some(e.chars().take(300).collect()); }
                }
            }
            running.store(false, Ordering::SeqCst);
        });
        true
    }
}

/// Keeps a project made before the Channels step looking the same: where its old fixed rules
/// named a channel differently from today's guess, that becomes a saved choice.
pub fn migrate_channels(dir: &Path) {
    let file = dir.join("channels.json");
    if file.exists() {
        return;
    }
    let index = read_json(&dir.join("index.json"), json!({}));
    let Ok(slices) = serde_json::from_value::<Vec<Slice>>(index["slices"].clone()) else { return };
    let mut map = Map::new();
    for s in &slices {
        for c in &s.channels {
            let guess = guess_stain(&c.filter, &s.stains);
            if guess != c.stain && !map.contains_key(&guess) {
                map.insert(guess, json!({"stain": c.stain}));
            }
        }
    }
    if !map.is_empty() {
        let _ = save_json(&file, &Value::Object(map));
    }
}

fn build(dir: &Path, root: &str, rerender_only: bool, set: &dyn Fn(&str, usize, &str), done: &AtomicUsize) -> Result<(), String> {
    let cache = dir.join("cache");
    let img_dir = dir.join("images");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    fs::create_dir_all(&img_dir).map_err(|e| e.to_string())?;
    let index_file = dir.join("index.json");
    let labels_file = dir.join("labels.json");
    migrate_channels(dir);
    let mut slices: Vec<Slice>;
    if rerender_only && index_file.exists() {
        slices = serde_json::from_value(read_json(&index_file, json!({}))["slices"].clone()).map_err(|e| e.to_string())?;
    } else {
        if root.is_empty() {
            return Err("Choose your image folder first.".into());
        }
        if !Path::new(root).is_dir() {
            return Err(format!("Can't find the image folder ({root}). If it's on the lab drive, connect the VPN and the drive, then try again."));
        }
        set("scanning", 0, "Searching the image folder…");
        slices = if labels_file.exists() {
            let det: Detection = serde_json::from_value(read_json(&labels_file, json!({}))).map_err(|e| e.to_string())?;
            slices_from_labels(&det)
        } else {
            scan_folder(Path::new(root))
        };
        if slices.is_empty() {
            return Err("No images to show yet. Open Labels & channels to check which files are included.".into());
        }
        set("copying", slices.len(), "");
        let prepared: Vec<Result<Slice, String>> = slices.into_par_iter().map(|mut s| {
            prepare_cache(&mut s, dir).map_err(|e| format!("Preparing {}: {e}", s.id))?;
            done.fetch_add(1, Ordering::Relaxed);
            Ok(s)
        }).collect();
        slices = prepared.into_iter().collect::<Result<_, _>>()?;
    }
    // Read each slice's channels and pool pixel statistics per stain for the auto contrast
    set("measuring", slices.len(), "Measuring brightness per stain");
    let choices = channel_choices(dir);
    let pooled: Mutex<BTreeMap<String, Vec<f64>>> = Mutex::new(BTreeMap::new());
    let rgb_stains: Mutex<Vec<String>> = Mutex::new(vec![]);
    let measured: Vec<Result<(Vec<Channel>, [u32; 2], Option<f64>), String>> = slices.par_iter().map(|s| {
        let ch = read_channels(&s.cache_path(dir))?;
        let chans = channels_for(&ch.filters, s, &choices);
        if ch.rgb {
            rgb_stains.lock().unwrap().extend(chans.iter().map(|c| c.stain.clone()));
        }
        for (px, c) in ch.data.iter().zip(&chans) {
            // Every slice counts the same, whatever its size (like sampling 40,000 pixels from each)
            let weight = (40000.0f64).min(px.len() as f64) / px.len().max(1) as f64;
            let h = histogram(px);
            let mut p = pooled.lock().unwrap();
            let acc = p.entry(c.stain.clone()).or_insert_with(|| vec![0.0; 65536]);
            for (a, n) in acc.iter_mut().zip(h) {
                *a += n as f64 * weight;
            }
        }
        done.fetch_add(1, Ordering::Relaxed);
        Ok((chans, [ch.w, ch.h], ch.um))
    }).collect();
    for (s, m) in slices.iter_mut().zip(measured) {
        let (chans, size, um) = m.map_err(|e| format!("Reading {}: {e}", s.id))?;
        s.channels = chans;
        s.preview_size = size;
        // keep a full-resolution pixel size known from the labels; otherwise the preview's (the
        // slide scanner writes the full-resolution size into every size's file)
        s.um_per_px = s.um_per_px.or(um);
        if s.full_size == [0, 0] {
            // the slide scanner's sizes halve each step
            let f = 2u32.pow(s.preview_level.saturating_sub(1));
            s.full_size = [size[0] * f, size[1] * f];
        }
    }
    let contrast_file = dir.join("contrast.json");
    let mut contrast = read_json(&contrast_file, json!({}));
    if !contrast.is_object() {
        contrast = json!({});
    }
    let rgb_stains = rgb_stains.into_inner().unwrap();
    // Most of each image is empty slide glowing faintly in the mounting medium, so black is set a
    // little above the typical (median) pixel, which hides that glow. Colour pictures keep their
    // full range.
    for (stain, hist) in pooled.into_inner().unwrap() {
        if !contrast[&stain].is_object() || contrast[&stain]["auto"].as_bool().unwrap_or(false) {
            // brightfield is a picture of light, not a glow: keep its full range too
            contrast[&stain] = if rgb_stains.contains(&stain) || stain.eq_ignore_ascii_case("BF") {
                json!({"min": 0.0, "max": 65535.0, "gamma": 1.0, "show": true, "auto": true})
            } else {
                json!({"min": percentile(&hist, 60.0), "max": percentile(&hist, 99.8), "gamma": 1.0, "show": true, "auto": true})
            };
        }
    }
    save_json(&contrast_file, &contrast).map_err(|e| e.to_string())?;
    set("rendering", slices.len(), "Making the images");
    slices.par_iter().try_for_each(|s| {
        let ch = read_channels(&s.cache_path(dir)).map_err(|e| format!("Reading {}: {e}", s.id))?;
        render(s, &ch, &contrast, &img_dir)?;
        done.fetch_add(1, Ordering::Relaxed);
        Ok::<(), String>(())
    })?;
    let folder = if rerender_only { read_json(&index_file, json!({}))["imageFolder"].clone() } else { json!(root) };
    save_json(&index_file, &json!({"builtAt": now(), "imageFolder": folder, "slices": slices})).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Re-renders a few images of a real project into a temp folder and compares them with the
    /// JPEGs the app made before. Set FLUORA_TEST_PROJECT to a project folder and run with --ignored.
    #[test]
    #[ignore]
    fn renders_like_before() {
        let Ok(proj) = std::env::var("FLUORA_TEST_PROJECT").map(PathBuf::from) else { return };
        let index = read_json(&proj.join("index.json"), json!({}));
        let slices: Vec<Slice> = serde_json::from_value(index["slices"].clone()).unwrap();
        let contrast = read_json(&proj.join("contrast.json"), json!({}));
        let out = std::env::temp_dir().join("fluora-render-check");
        fs::create_dir_all(&out).unwrap();
        migrate_channels(&proj);
        let choices = channel_choices(&proj);
        for s in slices.iter().step_by(97) {
            let ch = read_channels(&s.cache_path(&proj)).unwrap();
            let mut s2 = s.clone();
            s2.channels = channels_for(&ch.filters, s, &choices);
            assert_eq!(s2.channels.iter().map(|c| &c.stain).collect::<Vec<_>>(), s.channels.iter().map(|c| &c.stain).collect::<Vec<_>>(), "{}", s.id);
            render(&s2, &ch, &contrast, &out).unwrap();
            let a = image::open(proj.join("images").join(format!("{}_merged.jpg", s.id))).unwrap().to_rgb8();
            let b = image::open(out.join(format!("{}_merged.jpg", s.id))).unwrap().to_rgb8();
            assert_eq!(a.dimensions(), b.dimensions());
            let diff: f64 = a.as_raw().iter().zip(b.as_raw()).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.as_raw().len() as f64;
            println!("{} {:?} mean diff {diff:.3}", s.id, s2.channels.iter().map(|c| &c.stain).collect::<Vec<_>>());
            assert!(diff < 1.0);
        }
    }
}
