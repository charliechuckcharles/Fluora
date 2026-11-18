//! The lab's file-name convention.
//!
//!   image:    [Sample]_Cond_SL##_IM##_Stains_G[x]_Obj_[Focus]_Date_Initials.tif
//!   CZI:      [Sample]_Cond_SL##_Stains_Obj_[Focus]_Date_Initials.czi
//!   label:    [Sample]_Cond_SL##_SlideLabel_Date_Initials.tif   (and SlideOverview)
//!
//! G[x] says which part of a set the file is: G[R1], G[R2]... are the sizes of one image
//! (R1 = full resolution), and G[DAPI], G[BF]... are single-stain files of one field of view.
//! Sample and Focus are optional. Fields never contain "_", which separates them.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Fields {
    pub sample: String,
    pub cond: String,
    pub slide: String,
    pub im: String,
    pub stains: String,
    pub obj: String,
    pub focus: String,
    pub date: String,
    pub initials: String,
}

/// The fields every image name needs, in name order.
pub const REQUIRED: [&str; 7] = ["cond", "slide", "im", "stains", "obj", "date", "initials"];
pub const ALL: [&str; 9] = ["sample", "cond", "slide", "im", "stains", "obj", "focus", "date", "initials"];

impl Fields {
    pub fn get(&self, k: &str) -> &str {
        match k {
            "sample" => &self.sample,
            "cond" => &self.cond,
            "slide" => &self.slide,
            "im" => &self.im,
            "stains" => &self.stains,
            "obj" => &self.obj,
            "focus" => &self.focus,
            "date" => &self.date,
            "initials" => &self.initials,
            _ => "",
        }
    }
    pub fn set(&mut self, k: &str, v: String) {
        match k {
            "sample" => self.sample = v,
            "cond" => self.cond = v,
            "slide" => self.slide = v,
            "im" => self.im = v,
            "stains" => self.stains = v,
            "obj" => self.obj = v,
            "focus" => self.focus = v,
            "date" => self.date = v,
            "initials" => self.initials = v,
            _ => {}
        }
    }
    /// These fields, with any blank ones taken from `base`.
    pub fn over(&self, base: &Fields) -> Fields {
        let mut out = base.clone();
        for k in ALL {
            if !self.get(k).trim().is_empty() {
                out.set(k, self.get(k).to_string());
            }
        }
        out
    }
    /// Required fields that are blank (for an image; labels and CZIs need fewer).
    pub fn missing(&self, what: Special) -> Vec<&'static str> {
        let need: &[&str] = match what {
            Special::None => &REQUIRED,
            Special::Czi => &["cond", "slide", "stains", "obj", "date", "initials"],
            Special::Label | Special::Overview => &["cond", "slide", "date", "initials"],
        };
        need.iter().copied().filter(|k| clean(self.get(k)).is_empty()).collect()
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Special {
    #[default]
    None,
    Czi,
    Label,
    Overview,
}

#[derive(Clone, Debug, Default)]
pub struct Parsed {
    pub fields: Fields,
    /// What is inside G[...], e.g. "R1" or "DAPI"
    pub group: Option<String>,
    pub special: Special,
}

fn re(pat: &'static str, cell: &'static OnceLock<Regex>) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}
static SL: OnceLock<Regex> = OnceLock::new();
static IM: OnceLock<Regex> = OnceLock::new();
static G: OnceLock<Regex> = OnceLock::new();
static OBJ: OnceLock<Regex> = OnceLock::new();
static DATE: OnceLock<Regex> = OnceLock::new();
static INIT: OnceLock<Regex> = OnceLock::new();

pub fn stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// Reads the fields out of a file name that follows the convention (None if it doesn't).
pub fn parse(file_name: &str) -> Option<Parsed> {
    let lower = file_name.to_lowercase();
    let is_czi = lower.ends_with(".czi");
    let st = stem(file_name);
    let st = st.strip_suffix(".ome").unwrap_or(st);
    let tokens: Vec<&str> = st.split('_').collect();
    let sl = tokens.iter().position(|t| re(r"^(?i)SL\d+$", &SL).is_match(t))?;
    let mut p = Parsed::default();
    let f = &mut p.fields;
    let pre = &tokens[..sl];
    match pre.len() {
        0 => {}
        1 => f.cond = pre[0].into(),
        n => {
            f.sample = pre[..n - 1].join("_");
            f.cond = pre[n - 1].into();
        }
    }
    f.slide = tokens[sl][2..].to_string();
    let mut i = sl + 1;
    if let Some(t) = tokens.get(i) {
        if t.eq_ignore_ascii_case("SlideLabel") || t.eq_ignore_ascii_case("SlideOverview") {
            p.special = if t.eq_ignore_ascii_case("SlideLabel") { Special::Label } else { Special::Overview };
            i += 1;
        } else if re(r"^(?i)IM\d+$", &IM).is_match(t) {
            f.im = t[2..].to_string();
            i += 1;
        } else if !is_czi {
            // an image name always has IM##
            return None;
        }
    }
    if is_czi {
        p.special = Special::Czi;
    }
    if p.special == Special::None || p.special == Special::Czi {
        if let Some(t) = tokens.get(i) {
            if !re(r"^G\[(.+)\]$", &G).is_match(t) {
                f.stains = t.to_string();
                i += 1;
            }
        }
    }
    if let Some(t) = tokens.get(i) {
        if let Some(c) = re(r"^G\[(.+)\]$", &G).captures(t) {
            p.group = Some(c[1].to_string());
            i += 1;
        }
    }
    let rest = &tokens[i.min(tokens.len())..];
    let mut focus: Vec<&str> = vec![];
    for (j, t) in rest.iter().enumerate() {
        let last = j == rest.len() - 1;
        if f.obj.is_empty() && re(r"^(?i)\d+(\.\d+)?x$", &OBJ).is_match(t) {
            f.obj = t.to_string();
        } else if f.date.is_empty() && re(r"^\d{8}$", &DATE).is_match(t) {
            f.date = t.to_string();
        } else if last && !f.date.is_empty() && re(r"^[A-Za-z]{1,6}$", &INIT).is_match(t) {
            f.initials = t.to_string();
        } else {
            focus.push(t);
        }
    }
    f.focus = focus.join("-");
    Some(p)
}

/// A field value made safe for a file name: no "_" (it separates fields), no characters
/// Windows or macOS refuse, no spaces.
pub fn clean(v: &str) -> String {
    let mut s: String = v.trim().chars().map(|c| match c {
        '_' | ' ' | ',' | ';' | '+' | '/' | '\\' => '-',
        c => c,
    }).filter(|c| !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|' | '[' | ']') && !c.is_control()).collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s.trim_matches(|c| c == '-' || c == '.').to_string()
}

fn num(prefix: &str, v: &str) -> String {
    let v = clean(v);
    // "SL47" typed into the slide box means 47
    let v = if v.len() > prefix.len() && v[..prefix.len()].eq_ignore_ascii_case(prefix) { &v[prefix.len()..] } else { &v[..] };
    if v.is_empty() {
        return String::new();
    }
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) && v.len() < 2 {
        format!("{prefix}{v:0>2}")
    } else {
        format!("{prefix}{v}")
    }
}

fn join(parts: Vec<String>) -> String {
    parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("_")
}

/// The convention name for one file of an image. `group` is what goes in G[...], if anything.
pub fn image_name(f: &Fields, group: Option<&str>, ext: &str) -> String {
    let g = group.map(|g| format!("G[{}]", clean(g))).unwrap_or_default();
    format!("{}.{ext}", join(vec![clean(&f.sample), clean(&f.cond), num("SL", &f.slide), num("IM", &f.im), clean(&f.stains), g,
        clean(&f.obj), clean(&f.focus), clean(&f.date), clean(&f.initials)]))
}

pub fn czi_name(f: &Fields) -> String {
    format!("{}.czi", join(vec![clean(&f.sample), clean(&f.cond), num("SL", &f.slide), clean(&f.stains), clean(&f.obj),
        clean(&f.focus), clean(&f.date), clean(&f.initials)]))
}

pub fn special_name(f: &Fields, what: Special, ext: &str) -> String {
    let label = if what == Special::Label { "SlideLabel" } else { "SlideOverview" };
    format!("{}.{ext}", join(vec![clean(&f.sample), clean(&f.cond), num("SL", &f.slide), label.into(), clean(&f.date), clean(&f.initials)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_old_and_new_names() {
        let p = parse("01f_C_SL01_IM01_DAPI-GAF_G[R1]_WB_20250730_CNH.tif").unwrap();
        assert_eq!(p.fields.sample, "01f");
        assert_eq!(p.fields.cond, "C");
        assert_eq!(p.fields.slide, "01");
        assert_eq!(p.fields.im, "01");
        assert_eq!(p.fields.stains, "DAPI-GAF");
        assert_eq!(p.group.as_deref(), Some("R1"));
        assert_eq!(p.fields.focus, "WB");
        assert_eq!(p.fields.date, "20250730");
        assert_eq!(p.fields.initials, "CNH");
        let p = parse("03p_Cad_SL47_IM01_BF-DAPI_G[BF]_20x_HD_20260419_CNH.tif").unwrap();
        assert_eq!(p.group.as_deref(), Some("BF"));
        assert_eq!(p.fields.obj, "20x");
        assert_eq!(p.fields.focus, "HD");
        let p = parse("E_SL3_IM2_DAPI_10x_20260101_AB.tif").unwrap();
        assert_eq!(p.fields.sample, "");
        assert_eq!(p.fields.cond, "E");
        assert_eq!(p.group, None);
        let p = parse("01f_C_SL01_DAPI-GAF_20x_WB_20250730_CNH.czi").unwrap();
        assert_eq!(p.special, Special::Czi);
        assert_eq!(p.fields.stains, "DAPI-GAF");
        assert_eq!(p.fields.obj, "20x");
        let p = parse("01o_E_SL36_SlideLabel_20250730_CNH.tif").unwrap();
        assert_eq!(p.special, Special::Label);
        assert!(parse("Snap-111_series0.tif").is_none());
        assert!(parse("01o_E_SL36_DAPI_x.tif").is_none());
    }

    #[test]
    fn builds_names() {
        let f = Fields { sample: "03p".into(), cond: "Cad".into(), slide: "47".into(), im: "1".into(), stains: "BF, DAPI".into(),
            obj: "20x".into(), focus: "HD".into(), date: "20260419".into(), initials: "CNH".into() };
        assert_eq!(image_name(&f, Some("BF"), "tif"), "03p_Cad_SL47_IM01_BF-DAPI_G[BF]_20x_HD_20260419_CNH.tif");
        let g = Fields { sample: "".into(), focus: "".into(), ..f.clone() };
        assert_eq!(image_name(&g, Some("R2"), "tif"), "Cad_SL47_IM01_BF-DAPI_G[R2]_20x_20260419_CNH.tif");
        assert_eq!(czi_name(&f), "03p_Cad_SL47_BF-DAPI_20x_HD_20260419_CNH.czi");
        assert_eq!(special_name(&f, Special::Overview, "tif"), "03p_Cad_SL47_SlideOverview_20260419_CNH.tif");
        // round trip
        let n = image_name(&f, Some("R1"), "tif");
        assert_eq!(parse(&n).unwrap().fields.over(&Fields::default()).stains, "BF-DAPI");
    }
}
