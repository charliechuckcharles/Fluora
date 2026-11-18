//! What a TIFF holds, read from its header only (no pixels), so it is quick even over the VPN.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tiff::decoder::{ifd::Value, Decoder, Limits};
use tiff::tags::Tag;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TiffInfo {
    pub w: u32,
    pub h: u32,
    /// Separate channels (pages, or samples for a grey multi-sample file)
    pub channels: u32,
    /// An 8-bit colour picture (brightfield, slide label, overview)
    pub rgb: bool,
    pub bits: u16,
    /// Channel (filter) names from the Zeiss / ImageJ / OME metadata
    pub filters: Vec<String>,
    /// Micrometres per pixel
    pub um: Option<f64>,
    /// The CZI this file was unpacked from, when Fluora's unpacker made it
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The ImageJ "Info" text (tag 50839): UTF-16 in the file's byte order ("IJIJ" = big-endian).
pub fn imagej_info(dec: &mut Decoder<BufReader<File>>) -> String {
    dec.get_tag_u8_vec(Tag::Unknown(50839)).ok().map(|b| {
        let be = b.starts_with(b"IJIJ");
        let units: Vec<u16> = b.chunks_exact(2).map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect();
        String::from_utf16_lossy(&units)
    }).unwrap_or_default()
}

/// Channel names, from Zeiss metadata kept by ImageJ, or from OME-XML.
pub fn channel_names(info: &str, description: &str) -> BTreeMap<usize, String> {
    let zeiss = Regex::new(r"Information\|Image\|Channel\|Name #(\d+) = ([^\n\r]*)").unwrap();
    let mut names: BTreeMap<usize, String> = zeiss.captures_iter(info)
        .filter_map(|c| Some((c[1].parse().ok()?, c[2].trim().to_string()))).collect();
    if names.is_empty() && description.contains("<OME") {
        let ch = Regex::new(r#"<Channel\b[^>]*?\bName="([^"]*)""#).unwrap();
        names = ch.captures_iter(description).enumerate().map(|(i, c)| (i + 1, c[1].to_string())).collect();
    }
    names
}

/// Pixel size: the TIFF resolution (ImageJ writes pixels per micrometre), else OME's PhysicalSizeX.
pub fn pixel_size(dec: &mut Decoder<BufReader<File>>, description: &str) -> Option<f64> {
    let micron = ["unit=micron", "unit=um", "unit=\\u00B5m", "unit=µm"].iter().any(|u| description.contains(u));
    if let Ok(Value::Rational(n, d)) = dec.get_tag(Tag::XResolution) {
        if n > 0 && d > 0 {
            let r = d as f64 / n as f64;
            if micron {
                return Some(r);
            }
            // ResolutionUnit 3 = centimetre
            if dec.get_tag_u32(Tag::ResolutionUnit).ok() == Some(3) && !description.contains("<OME") {
                return Some(r * 10000.0);
            }
        }
    }
    let re = Regex::new(r#"PhysicalSizeX="([0-9.eE+-]+)""#).unwrap();
    re.captures(description).and_then(|c| c[1].parse().ok())
}

pub fn read(path: &Path) -> Result<TiffInfo, String> {
    let f = File::open(path).map_err(|e| e.to_string())?;
    let mut dec = Decoder::new(BufReader::new(f)).map_err(|e| e.to_string())?.with_limits(Limits::unlimited());
    let (w, h) = dec.dimensions().map_err(|e| e.to_string())?;
    let spp = dec.get_tag_u32(Tag::SamplesPerPixel).unwrap_or(1).max(1);
    let bits = dec.get_tag_u16_vec(Tag::BitsPerSample).ok().and_then(|v| v.first().copied()).unwrap_or(8);
    let photometric = dec.get_tag_u32(Tag::PhotometricInterpretation).unwrap_or(1);
    let rgb = spp >= 3 && photometric == 2;
    let description = dec.get_tag_ascii_string(Tag::ImageDescription).unwrap_or_default();
    let info = imagej_info(&mut dec);
    let um = pixel_size(&mut dec, &description);
    let from_desc = |key: &str| description.lines().find_map(|l| l.strip_prefix(key).and_then(|v| v.trim().parse::<u32>().ok()));
    let channels = if rgb {
        1
    } else if spp > 1 {
        spp
    } else if let Some(c) = from_desc("channels=") {
        c
    } else if let Some(c) = Regex::new(r#"SizeC="(\d+)""#).unwrap().captures(&description).and_then(|c| c[1].parse().ok()) {
        c
    } else {
        // count same-size pages, at most a handful
        let mut n = 1;
        while n < 16 && dec.more_images() && dec.next_image().is_ok() {
            if dec.dimensions().ok() != Some((w, h)) {
                break;
            }
            n += 1;
        }
        n
    };
    let names = channel_names(&info, &description);
    let filters = (0..channels as usize).map(|i| names.get(&(i + 1)).cloned().unwrap_or_default()).collect();
    let source = Regex::new(r"Information\|Fluora\|SourceCzi = ([^\n\r]+)").unwrap().captures(&info).map(|c| c[1].trim().to_string());
    Ok(TiffInfo { w, h, channels, rgb, bits, filters, um, source })
}
