// The setup steps for a project: find the images in a folder, unpack CZIs with Fiji, label the
// images, rename files on the drive (optional, with undo), and choose stain names and colours.
//
// The slow parts (looking through a big folder over the VPN, renaming, unpacking) run in the
// background as one "job" at a time; the app polls /api/setup/status for progress.
use crate::slides;
use crate::store::{read_json, save_json};
pub use fluora_naming::tasks::*;
use fluora_naming::Kind;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;


// ---------------------------------------------------------------- channels

/// Every stain Fluora would show, with the filters it comes from and how many images have it,
/// and the project's saved choice of name and colour.
pub fn channels(labels: &Value, project: &Path) -> Result<Value, String> {
    let det = labels_from(labels)?;
    let choices = slides::channel_choices(project);
    let mut found: BTreeMap<String, (Vec<String>, usize)> = BTreeMap::new();
    for g in det.groups.iter().filter(|g| !g.skip) {
        for e in g.entries.iter().filter(|e| e.kind == Kind::Image) {
            let f = g.fields_of(e);
            let stains: Vec<String> = f.stains.split(['-', ',', ' ']).filter(|x| !x.is_empty()).map(|x| x.to_uppercase()).collect();
            let filters: Vec<String> = if e.split {
                e.parts.iter().map(|p| p.tag.clone()).collect()
            } else {
                e.parts.first().and_then(|p| p.file.info.as_ref()).map(|i| i.filters.clone()).filter(|v| !v.is_empty())
                    .unwrap_or_else(|| g.filters.clone())
            };
            let filters: Vec<String> = filters.into_iter().enumerate().map(|(i, x)| if x.is_empty() { format!("Ch{}", i + 1) } else { x }).collect();
            for flt in filters {
                let guess = if e.split { flt.clone() } else { slides::guess_stain(&flt, &stains) };
                let entry = found.entry(guess).or_default();
                if !entry.0.contains(&flt) {
                    entry.0.push(flt);
                }
                entry.1 += 1;
            }
        }
    }
    let list: Vec<Value> = found.into_iter().map(|(guess, (filters, n))| {
        let (stain, color) = choices.get(&guess).cloned().unwrap_or((guess.clone(), None));
        let color = color.unwrap_or_else(|| slides::color_for(&stain, filters.first().map(|s| s.as_str()).unwrap_or("")));
        json!({"guess": guess, "filters": filters, "images": n, "stain": stain, "color": slides::hex(color)})
    }).collect();
    Ok(json!(list))
}

/// Saves the labels and channel choices for the project.
pub fn save(project: &Path, labels: &Value, channels: &Value) -> Result<(), String> {
    let det = labels_from(labels)?;
    save_json(&project.join("labels.json"), &serde_json::to_value(&det).unwrap()).map_err(|e| e.to_string())?;
    let mut map = serde_json::Map::new();
    for c in channels.as_array().cloned().unwrap_or_default() {
        let (Some(guess), Some(stain)) = (c["guess"].as_str(), c["stain"].as_str()) else { continue };
        let stain = stain.trim();
        let mut o = json!({"stain": if stain.is_empty() { guess } else { stain }});
        if let Some(col) = c["color"].as_str().filter(|x| slides::parse_hex(x).is_some()) {
            o["color"] = json!(col);
        }
        map.insert(guess.to_string(), o);
    }
    save_json(&project.join("channels.json"), &Value::Object(map)).map_err(|e| e.to_string())
}

/// What the setup screens start from: the project's saved labels, if any.
pub fn load(project: &Path) -> Value {
    let labels = read_json(&project.join("labels.json"), Value::Null);
    json!({"labels": labels})
}
