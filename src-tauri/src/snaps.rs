// Snaps: zoomed-in pictures of a slice, taken from the viewer, with the scale bar burned in.
//
// Each snap is an ordinary PNG in the project's snaps/ folder, named so you can tell what it is
// in Finder (e.g. Slide01_IM05_Phalloidin_20261115-203112.png). Everything about it (which slide
// and image, the stain, where on the slice, the pixel size, who took it) is also written inside
// the PNG, so the information travels with the file. snaps.json lists them for the Snaps tab.
use crate::slides::{self, Slice};
use crate::tiffread;
use crate::store::{now, read_json, save_json};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub fn dir(project: &Path) -> PathBuf {
    project.join("snaps")
}

pub fn list(project: &Path) -> Value {
    let v = read_json(&dir(project).join("snaps.json"), json!([]));
    if v.is_array() { v } else { json!([]) }
}

fn save_list(project: &Path, v: &Value) -> std::io::Result<()> {
    save_json(&dir(project).join("snaps.json"), v)
}

/// What the snap is made from: the pixels of the zoomed-in part of the slice, at the coarsest
/// scanner size that still has at least `want` pixels across, read from the image folder when
/// it's reachable and from the local preview otherwise.
pub struct Region {
    pub png: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub level: u32,
    pub um_per_px: Option<f64>,
    pub source: &'static str,
}

pub fn region(project: &Path, s: &Slice, contrast: &Value, frac: [f64; 4], channel: &str, want: f64, bright: f32) -> Result<Region, String> {
    let [pw, ph] = s.preview_size;
    let full_w = if s.full_size[0] > 0 { s.full_size[0] } else { pw };
    // Sizes sharper than the preview, coarsest first; stop at the first that's sharp enough
    let mut finer: Vec<(u32, [u32; 2])> = if s.split.is_empty() {
        s.levels.keys().map(|l| (*l, s.size_of(*l))).filter(|(_, sz)| sz[0] > pw).collect()
    } else {
        vec![(1, s.full_size)].into_iter().filter(|(_, sz)| sz[0] > pw).collect()
    };
    finer.sort_unstable_by_key(|(_, sz)| sz[0]);
    let long_at = |sz: [u32; 2]| (frac[2] * sz[0] as f64).max(frac[3] * sz[1] as f64);
    let px_at = |sz: [u32; 2]| frac[2] * sz[0] as f64 * frac[3] * sz[1] as f64;
    let mut pick: Option<(u32, [u32; 2])> = None;
    if long_at([pw, ph]) < want {
        for c in finer {
            if px_at(c.1) > 60e6 {
                break; // more than ~60 megapixels: too big to be worth it
            }
            pick = Some(c);
            if long_at(c.1) >= want {
                break;
            }
        }
    }
    let only = if channel == "merged" { None } else { Some(channel) };
    let preview = || tiffread::read_window(&s.cache_path(project), Some(frac), 1);
    let (ch, level, width, source) = match pick {
        Some((l, sz)) => {
            let read = if s.split.is_empty() {
                s.levels.get(&l).map(PathBuf::from).filter(|p| p.exists()).map(|p| tiffread::read_window(&p, Some(frac), 1))
            } else if s.split.values().all(|p| Path::new(p).exists()) {
                Some(slides::read_split(s, Some(frac), 1))
            } else {
                None
            };
            match read {
                Some(r) => (r?, l, sz[0], "full"),
                None => (preview()?, s.preview_level, pw, "preview"),
            }
        }
        None => (preview()?, s.preview_level, pw, "preview-enough"),
    };
    let mut ch = ch;
    // Use the channel names found when the slides were scanned, in case a size lacks them
    if ch.data.len() == s.channels.len() {
        ch.filters = s.channels.iter().map(|c| c.filter.clone()).collect();
    }
    let rgb = slides::compose(s, &ch, contrast, only, bright);
    let mut png = vec![];
    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Adaptive)
        .write_image(&rgb, ch.w, ch.h, ExtendedColorType::Rgb8).map_err(|e| e.to_string())?;
    // µm per pixel of what was read: the full-resolution size scaled by how much smaller it is
    let um = s.um_per_px.map(|u| u * full_w as f64 / width.max(1) as f64);
    Ok(Region { png, w: ch.w, h: ch.h, level, um_per_px: um, source })
}

// ---------------------------------------------------------------- saving

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in bytes {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// Adds UTF-8 text (iTXt) to a PNG, just before its end, so the snap's details live in the file.
fn with_text(png: &[u8], fields: &[(&str, String)]) -> Result<Vec<u8>, String> {
    if png.len() < 20 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("Not a PNG".into());
    }
    let iend = png.len() - 12;
    if &png[iend + 4..iend + 8] != b"IEND" {
        return Err("PNG has no end marker".into());
    }
    let mut out = png[..iend].to_vec();
    for (key, text) in fields {
        let mut data = key.as_bytes().to_vec();
        data.extend([0, 0, 0, 0, 0]); // null, no compression, method 0, empty language, empty translated key
        data.extend(text.as_bytes());
        let mut chunk = b"iTXt".to_vec();
        chunk.extend(&data);
        out.extend((data.len() as u32).to_be_bytes());
        out.extend(&chunk);
        out.extend(crc32(&chunk).to_be_bytes());
    }
    out.extend(&png[iend..]);
    Ok(out)
}

fn clean(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

/// Saves a snap the viewer made (a PNG with the scale bar drawn in) with its details.
pub fn save(project: &Path, png: &[u8], meta: Value, by: &str, project_name: &str) -> Result<Value, String> {
    let d = dir(project);
    fs::create_dir_all(d.join("thumbs")).map_err(|e| e.to_string())?;
    let slide = meta["slide"].as_u64().unwrap_or(0);
    let im = meta["im"].as_u64().unwrap_or(0);
    let chan = clean(meta["channelLabel"].as_str().unwrap_or("Merged"));
    let stamp = clean(meta["stamp"].as_str().unwrap_or(""));
    let mut base = format!("Slide{slide:02}_IM{im:02}_{chan}_{stamp}");
    let mut n = 1;
    while d.join(format!("{base}.png")).exists() {
        n += 1;
        base = format!("Slide{slide:02}_IM{im:02}_{chan}_{stamp}-{n}");
    }
    let id = format!("s{}", (now() * 1000.0) as u64);
    let file = format!("{base}.png");
    let mut rec = meta.clone();
    rec["id"] = json!(id);
    rec["file"] = json!(file);
    rec["thumb"] = json!(format!("thumbs/{id}.jpg"));
    rec["created"] = json!(now());
    rec["by"] = json!(by);
    rec["project"] = json!(project_name);
    if !rec["caption"].is_string() {
        rec["caption"] = json!("");
    }
    if let Some(o) = rec.as_object_mut() {
        o.remove("stamp");
    }
    let describe = format!(
        "Slide {slide}, image {im} ({}), {}. {} wide. Taken by {} in Fluora.",
        [rec["block"].as_str().unwrap_or(""), rec["condition"].as_str().unwrap_or("")].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · "),
        rec["channelLabel"].as_str().unwrap_or("Merged"),
        rec["widthUm"].as_f64().map(|w| if w >= 1000.0 { format!("{:.2} mm", w / 1000.0) } else { format!("{w:.0} µm") }).unwrap_or("?".into()),
        if by.is_empty() { "someone" } else { by });
    let bytes = with_text(png, &[("Title", base.clone()), ("Description", describe), ("Software", "Fluora".into()),
        ("SlideBrowser", serde_json::to_string(&rec).unwrap_or_default())])?;
    fs::write(d.join(&file), &bytes).map_err(|e| e.to_string())?;
    // A small JPEG for the Snaps tab
    if let Ok(img) = image::load_from_memory(png) {
        img.thumbnail(640, 640).to_rgb8().save_with_format(d.join("thumbs").join(format!("{id}.jpg")), image::ImageFormat::Jpeg).ok();
    }
    let mut all = list(project);
    all.as_array_mut().unwrap().push(rec.clone());
    save_list(project, &all).map_err(|e| e.to_string())?;
    Ok(rec)
}

pub fn update(project: &Path, id: &str, fields: &Value) -> Result<Value, String> {
    let mut all = list(project);
    let mut found = None;
    for s in all.as_array_mut().unwrap().iter_mut().filter(|s| s["id"] == json!(id)) {
        if let Some(c) = fields["caption"].as_str() {
            s["caption"] = json!(c);
        }
        if let Some(b) = fields["pick"].as_bool() {
            s["pick"] = json!(b);
            if b {
                s["pickedAt"] = json!(now());
            }
        }
        found = Some(s.clone());
    }
    save_list(project, &all).map_err(|e| e.to_string())?;
    found.ok_or("No such snap".into())
}

/// Takes a snap out of the list; its file moves to snaps/removed/ rather than being deleted.
pub fn remove(project: &Path, id: &str) -> Result<(), String> {
    let d = dir(project);
    let mut all = list(project);
    let arr = all.as_array_mut().unwrap();
    if let Some(s) = arr.iter().find(|s| s["id"] == json!(id)) {
        if let Some(f) = s["file"].as_str() {
            fs::create_dir_all(d.join("removed")).ok();
            fs::rename(d.join(f), d.join("removed").join(f)).ok();
        }
        if let Some(t) = s["thumb"].as_str() {
            fs::remove_file(d.join(t)).ok();
        }
    }
    arr.retain(|s| s["id"] != json!(id));
    save_list(project, &all).map_err(|e| e.to_string())
}
