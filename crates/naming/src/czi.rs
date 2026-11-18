//! A little of what a Zeiss CZI file records about itself (when it was taken and with which
//! objective), read from the XML metadata the file carries. Only the header and that block are
//! read, so it is quick even over the VPN.
use regex::Regex;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CziMeta {
    /// YYYYMMDD
    pub date: Option<String>,
    /// e.g. "10x"
    pub objective: Option<String>,
}

pub fn read_meta(path: &Path) -> Option<CziMeta> {
    let mut f = File::open(path).ok()?;
    let mut head = [0u8; 100];
    f.read_exact(&mut head).ok()?;
    if &head[..10] != b"ZISRAWFILE" {
        return None;
    }
    // the file segment's data: version (4+4), reserved (4+4), two GUIDs (16+16), file part (4),
    // directory position (8), metadata position (8)
    let pos = i64::from_le_bytes(head[92..100].try_into().ok()?);
    f.seek(SeekFrom::Start(pos as u64)).ok()?;
    let mut seg = [0u8; 32 + 256];
    f.read_exact(&mut seg).ok()?;
    if &seg[..14] != b"ZISRAWMETADATA" {
        return None;
    }
    let size = i32::from_le_bytes(seg[32..36].try_into().ok()?).clamp(0, 64 << 20) as usize;
    let mut xml = vec![0u8; size];
    f.read_exact(&mut xml).ok()?;
    Some(parse(&String::from_utf8_lossy(&xml)))
}

pub fn parse(xml: &str) -> CziMeta {
    let date = Regex::new(r"<AcquisitionDateAndTime>(\d{4})-(\d{2})-(\d{2})").unwrap().captures(xml)
        .map(|c| format!("{}{}{}", &c[1], &c[2], &c[3]));
    // the objective the image was taken with, else the only one listed
    let mag = |block: &str| Regex::new(r"<NominalMagnification>([\d.]+)</NominalMagnification>").unwrap()
        .captures(block).and_then(|c| c[1].parse::<f64>().ok());
    let used = Regex::new(r#"<ObjectiveRef Id="([^"]+)""#).unwrap().captures(xml).map(|c| c[1].to_string());
    let m = used.and_then(|id| {
        let start = xml.find(&format!(r#"<Objective Id="{id}""#))?;
        let end = xml[start..].find("</Objective>").map(|e| start + e).unwrap_or(xml.len());
        mag(&xml[start..end])
    }).or_else(|| mag(xml));
    let objective = m.filter(|x| *x > 0.0).map(|x| if x.fract() == 0.0 { format!("{}x", x as u32) } else { format!("{x}x") });
    CziMeta { date, objective }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_date_and_the_objective_used() {
        let xml = r#"<Image><AcquisitionDateAndTime>2025-07-31T16:28:05Z</AcquisitionDateAndTime><ObjectiveSettings><ObjectiveRef Id="Objective:2" /></ObjectiveSettings></Image>
            <Objectives><Objective Id="Objective:1" Name="a"><NominalMagnification>10</NominalMagnification></Objective>
            <Objective Id="Objective:2" Name="b"><NominalMagnification>63.0</NominalMagnification></Objective></Objectives>"#;
        let m = super::parse(xml);
        assert_eq!(m.date.as_deref(), Some("20250731"));
        assert_eq!(m.objective.as_deref(), Some("63x"));
    }
}
