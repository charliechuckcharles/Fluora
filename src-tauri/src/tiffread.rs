// Reads any part of a TIFF, at full size or shrunk, without loading the whole image.
//
// Works for the slide scanner's uncompressed exports (only the rows needed are read from disk,
// so a zoomed-in part of a huge file comes back quickly even over the VPN) and for compressed
// or tiled TIFFs (only the strips or tiles that overlap are decoded). Shrinking averages k×k
// blocks a band of rows at a time, so a 40,000-pixel-wide single-size image can become a
// preview without ever being in memory at once.
use fluora_naming::tiffinfo;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use tiff::decoder::{ChunkType, Decoder, DecodingResult, Limits};
use tiff::tags::{ByteOrder, Tag};

pub struct Channels {
    /// One plane per channel, 16-bit (8-bit images are scaled up)
    pub data: Vec<Vec<u16>>,
    pub w: u32,
    pub h: u32,
    pub filters: Vec<String>,
    /// µm per pixel as written in the file
    pub um: Option<f64>,
    /// An 8-bit colour picture (its three planes are red, green, blue)
    pub rgb: bool,
}

struct Page {
    ifd: usize,
    spp: usize,
    bits: u16,
    compression: u32,
    predictor: u32,
    /// where each strip or tile is in the file, and how long it is
    chunk_offsets: Vec<u64>,
    chunk_counts: Vec<u64>,
    uncompressed_strips: bool,
    offsets: Vec<u64>,
    rows_per_strip: u32,
    chunk_w: u32,
    chunk_h: u32,
}

struct Reader {
    dec: Decoder<BufReader<File>>,
    raw: File,
    w: u32,
    big_endian: bool,
    /// decoded chunks of the current page, by chunk index
    cache: HashMap<u32, Vec<u16>>,
}

fn to_u16(r: DecodingResult) -> Result<Vec<u16>, String> {
    match r {
        DecodingResult::U16(v) => Ok(v),
        DecodingResult::U8(v) => Ok(v.into_iter().map(|x| (x as u16) << 8 | x as u16).collect()),
        _ => Err("Unsupported pixel type (needs 8- or 16-bit)".into()),
    }
}

impl Reader {
    /// Rows y0..y1, columns x0..x1 of one page, as one interleaved buffer (spp samples per pixel).
    fn rows(&mut self, p: &Page, y0: u32, y1: u32, x0: u32, x1: u32) -> Result<Vec<u16>, String> {
        let (w, spp) = (self.w as usize, p.spp);
        let rw = (x1 - x0) as usize;
        let mut out = Vec::with_capacity(rw * (y1 - y0) as usize * spp);
        if p.uncompressed_strips && (p.bits == 16 || p.bits == 8) {
            let bps = (p.bits / 8) as u64;
            let rps = p.rows_per_strip;
            let mut row = y0;
            while row < y1 {
                let strip = (row / rps) as usize;
                let end = ((strip as u32 + 1) * rps).min(y1);
                let line = w as u64 * spp as u64 * bps;
                let start = p.offsets.get(strip).ok_or("Bad strip table")? + (row - strip as u32 * rps) as u64 * line + x0 as u64 * spp as u64 * bps;
                // full-width: the rows are one block on disk; cropped: one read per row
                let full = rw == w;
                let want = if full { line * (end - row) as u64 } else { rw as u64 * spp as u64 * bps };
                for r in 0..if full { 1 } else { (end - row) as u64 } {
                    let mut buf = vec![0u8; want as usize];
                    self.raw.seek(SeekFrom::Start(start + r * line)).map_err(|e| e.to_string())?;
                    self.raw.read_exact(&mut buf).map_err(|e| e.to_string())?;
                    if bps == 2 {
                        let be = self.big_endian;
                        out.extend(buf.chunks_exact(2).map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }));
                    } else {
                        out.extend(buf.iter().map(|&x| (x as u16) << 8 | x as u16));
                    }
                }
                row = end;
            }
            return Ok(out);
        }
        // Compressed or tiled: decode the chunks that overlap
        let across = self.w.div_ceil(p.chunk_w);
        let (cy0, cy1) = (y0 / p.chunk_h, (y1 - 1) / p.chunk_h);
        let (cx0, cx1) = (x0 / p.chunk_w, (x1 - 1) / p.chunk_w);
        // forget chunks above this band
        let keep_from = cy0 * across;
        self.cache.retain(|k, _| *k >= keep_from);
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                let idx = cy * across + cx;
                if !self.cache.contains_key(&idx) {
                    let v = match self.dec.read_chunk(idx) {
                        Ok(r) => to_u16(r)?,
                        // some writers pad edge tiles in a way the decoder rejects; decode those ourselves
                        Err(e) => self.decode_chunk(p, idx).map_err(|e2| format!("{e} ({e2})"))?,
                    };
                    self.cache.insert(idx, v);
                }
            }
        }
        for y in y0..y1 {
            let cy = y / p.chunk_h;
            for cx in cx0..=cx1 {
                let idx = cy * across + cx;
                // a decoded chunk is as wide as its data (edge tiles are cut to the image)
                let (dw, _) = self.dec.chunk_data_dimensions(idx);
                let stride = dw as usize;
                let chunk = &self.cache[&idx];
                let ry = (y - cy * p.chunk_h) as usize;
                let (ax, bx) = ((cx * p.chunk_w).max(x0), ((cx + 1) * p.chunk_w).min(x1).min(self.w));
                let a = (ry * stride + (ax - cx * p.chunk_w) as usize) * spp;
                let b = (ry * stride + (bx - cx * p.chunk_w) as usize) * spp;
                out.extend_from_slice(chunk.get(a..b).ok_or("Short chunk")?);
            }
        }
        Ok(out)
    }
}

impl Reader {
    /// Decodes one strip or tile straight from the file (uncompressed or LZW, with or without
    /// the horizontal predictor), cut to the part inside the image.
    fn decode_chunk(&mut self, p: &Page, idx: u32) -> Result<Vec<u16>, String> {
        let (off, len) = (*p.chunk_offsets.get(idx as usize).ok_or("no offset")?, *p.chunk_counts.get(idx as usize).ok_or("no length")?);
        let mut raw = vec![0u8; len as usize];
        self.raw.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
        self.raw.read_exact(&mut raw).map_err(|e| e.to_string())?;
        let bytes = match p.compression {
            1 => raw,
            5 => {
                let mut d = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
                let mut out = vec![];
                let r = d.into_vec(&mut out).decode(&raw);
                if out.is_empty() {
                    return Err(format!("{:?}", r.status));
                }
                out
            }
            c => return Err(format!("compression {c} not supported")),
        };
        let bps = (p.bits / 8).max(1) as usize;
        let tiled = p.chunk_w < self.w;
        // a tile is stored at its full size even at the image's edge
        let full_w = if tiled { p.chunk_w } else { self.dec.chunk_data_dimensions(idx).0 } as usize;
        let (dw, dh) = self.dec.chunk_data_dimensions(idx);
        let row_len = full_w * p.spp;
        let mut v: Vec<u16> = if bps == 2 {
            let be = self.big_endian;
            bytes.chunks_exact(2).map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect()
        } else {
            bytes.iter().map(|&x| x as u16).collect()
        };
        if p.predictor == 2 {
            for row in v.chunks_mut(row_len) {
                for i in p.spp..row.len() {
                    row[i] = if bps == 2 { row[i].wrapping_add(row[i - p.spp]) } else { (row[i] + row[i - p.spp]) & 0xff };
                }
            }
        }
        if bps == 1 {
            v.iter_mut().for_each(|x| *x = *x << 8 | *x);
        }
        let mut out = Vec::with_capacity(dw as usize * dh as usize * p.spp);
        for y in 0..dh as usize {
            let a = y * row_len;
            out.extend_from_slice(v.get(a..a + dw as usize * p.spp).ok_or("chunk too short")?);
        }
        Ok(out)
    }
}

/// Reads the part of the image inside `frac` (x, y, width, height as fractions of the whole;
/// None = all of it), shrunk by `k` (1 = full size, 4 = every 4×4 block averaged into one pixel).
pub fn read_window(path: &Path, frac: Option<[f64; 4]>, k: u32) -> Result<Channels, String> {
    let k = k.max(1);
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let raw = File::open(path).map_err(|e| e.to_string())?;
    let mut dec = Decoder::new(BufReader::new(file)).map_err(|e| e.to_string())?.with_limits(Limits::unlimited());
    let (w, h) = dec.dimensions().map_err(|e| e.to_string())?;
    let description = dec.get_tag_ascii_string(Tag::ImageDescription).unwrap_or_default();
    let info = tiffinfo::imagej_info(&mut dec);
    let um = tiffinfo::pixel_size(&mut dec, &description);
    let n_channels: Option<usize> = description.lines().find_map(|l| l.strip_prefix("channels=").and_then(|v| v.trim().parse().ok()))
        .or_else(|| regex::Regex::new(r#"SizeC="(\d+)""#).unwrap().captures(&description).and_then(|c| c[1].parse().ok()));
    let big_endian = matches!(dec.byte_order(), ByteOrder::BigEndian);
    let [fx, fy, fw, fh] = frac.unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let x0 = ((fx.clamp(0.0, 1.0) * w as f64).floor() as u32).min(w - 1);
    let y0 = ((fy.clamp(0.0, 1.0) * h as f64).floor() as u32).min(h - 1);
    let x1 = (((fx + fw).clamp(0.0, 1.0) * w as f64).ceil() as u32).clamp(x0 + 1, w);
    let y1 = (((fy + fh).clamp(0.0, 1.0) * h as f64).ceil() as u32).clamp(y0 + 1, h);
    let (ow, oh) = ((x1 - x0).div_ceil(k), (y1 - y0).div_ceil(k));

    // The pages that are channels of this image (same size as the first)
    let mut pages = vec![];
    let mut rgb = false;
    let mut ifd = 0usize;
    loop {
        if dec.dimensions().map_err(|e| e.to_string())? == (w, h) {
            let spp = dec.get_tag_u32(Tag::SamplesPerPixel).unwrap_or(1).max(1) as usize;
            let bits = dec.get_tag_u16_vec(Tag::BitsPerSample).ok().and_then(|v| v.first().copied()).unwrap_or(16);
            if pages.is_empty() {
                rgb = spp >= 3 && dec.get_tag_u32(Tag::PhotometricInterpretation).unwrap_or(1) == 2;
            }
            let compression = dec.get_tag_u32(Tag::Compression).unwrap_or(1);
            let strips = matches!(dec.get_chunk_type(), ChunkType::Strip);
            let planar = dec.get_tag_u32(Tag::PlanarConfiguration).unwrap_or(1);
            if planar != 1 && spp > 1 {
                return Err("This TIFF stores colour planes separately, which Fluora can't read yet".into());
            }
            let (chunk_w, chunk_h) = dec.chunk_dimensions();
            let uncompressed_strips = compression == 1 && strips;
            let offsets = if uncompressed_strips { dec.get_tag_u64_vec(Tag::StripOffsets).unwrap_or_default() } else { vec![] };
            let (ot, ct) = if strips { (Tag::StripOffsets, Tag::StripByteCounts) } else { (Tag::TileOffsets, Tag::TileByteCounts) };
            let chunk_offsets = dec.get_tag_u64_vec(ot).unwrap_or_default();
            let chunk_counts = dec.get_tag_u64_vec(ct).unwrap_or_default();
            let predictor = dec.get_tag_u32(Tag::Predictor).unwrap_or(1);
            let rows_per_strip = dec.get_tag_u32(Tag::RowsPerStrip).unwrap_or(h).max(1).min(h);
            pages.push(Page { ifd, spp, bits, compression, predictor, chunk_offsets, chunk_counts, uncompressed_strips, offsets, rows_per_strip, chunk_w: chunk_w.max(1), chunk_h: chunk_h.max(1) });
        }
        let have: usize = pages.iter().map(|p| p.spp).sum();
        if rgb || n_channels.is_some_and(|n| have >= n) || (n_channels.is_none() && have >= 16) || !dec.more_images() {
            break;
        }
        dec.next_image().map_err(|e| e.to_string())?;
        ifd += 1;
    }
    let mut r = Reader { dec, raw, w, big_endian, cache: HashMap::new() };
    let mut data: Vec<Vec<u16>> = vec![];
    for p in &pages {
        r.dec.seek_to_image(p.ifd).map_err(|e| e.to_string())?;
        r.cache.clear();
        let spp = p.spp;
        let rw = (x1 - x0) as usize;
        let mut planes: Vec<Vec<u16>> = (0..spp).map(|_| Vec::with_capacity((ow * oh) as usize)).collect();
        if k == 1 {
            let buf = r.rows(p, y0, y1, x0, x1)?;
            if spp == 1 {
                planes[0] = buf;
            } else {
                for (i, v) in buf.into_iter().enumerate() {
                    planes[i % spp].push(v);
                }
            }
        } else {
            let mut sums = vec![0u64; ow as usize * spp];
            for oy in 0..oh {
                let (ya, yb) = (y0 + oy * k, (y0 + (oy + 1) * k).min(y1));
                sums.iter_mut().for_each(|s| *s = 0);
                let buf = r.rows(p, ya, yb, x0, x1)?;
                for (row, line) in buf.chunks_exact(rw * spp).enumerate() {
                    let _ = row;
                    for x in 0..rw {
                        let o = (x / k as usize) * spp;
                        for c in 0..spp {
                            sums[o + c] += line[x * spp + c] as u64;
                        }
                    }
                }
                let rows = (yb - ya) as u64;
                for ox in 0..ow as usize {
                    let cols = ((ox + 1) * k as usize).min(rw) - ox * k as usize;
                    let n = rows * cols as u64;
                    for c in 0..spp {
                        planes[c].push((sums[ox * spp + c] / n.max(1)) as u16);
                    }
                }
            }
        }
        data.extend(planes);
        if rgb {
            data.truncate(3);
            break;
        }
    }
    if data.is_empty() {
        return Err("No image in the file".into());
    }
    let names = tiffinfo::channel_names(&info, &description);
    let filters = (0..data.len()).map(|i| {
        if rgb {
            ["Red", "Green", "Blue"][i.min(2)].to_string()
        } else {
            names.get(&(i + 1)).cloned().unwrap_or(format!("Ch{}", i + 1))
        }
    }).collect();
    Ok(Channels { data, w: ow, h: oh, filters, um, rgb })
}

/// Writes channels as a multi-page 16-bit TIFF with ImageJ-style channel names and pixel size,
/// which read_window reads back the same way.
pub fn write_cache(path: &Path, ch: &Channels) -> Result<(), String> {
    use tiff::encoder::{colortype, Rational, TiffEncoder};
    let part = path.with_extension("tif.part");
    {
        let f = std::io::BufWriter::new(File::create(&part).map_err(|e| e.to_string())?);
        let mut enc = TiffEncoder::new(f).map_err(|e| e.to_string())?;
        let desc = format!("ImageJ=1.54p\nimages={n}\nchannels={n}\nmode=color\n{unit}", n = ch.data.len(),
            unit = if ch.um.is_some() { "unit=micron\n" } else { "" });
        let names: String = ch.filters.iter().enumerate().map(|(i, f)| format!("Information|Image|Channel|Name #{} = {f}\n", i + 1)).collect();
        let info: Vec<u8> = names.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        for (i, plane) in ch.data.iter().enumerate() {
            let mut im = enc.new_image::<colortype::Gray16>(ch.w, ch.h).map_err(|e| e.to_string())?;
            if i == 0 {
                im.encoder().write_tag(Tag::ImageDescription, desc.as_str()).map_err(|e| e.to_string())?;
                im.encoder().write_tag(Tag::Unknown(50839), &info[..]).map_err(|e| e.to_string())?;
            }
            if let Some(um) = ch.um {
                let r = Rational { n: 1_000_000, d: (um * 1_000_000.0).round().max(1.0) as u32 };
                im.resolution(tiff::tags::ResolutionUnit::None, r);
            }
            im.write_data(plane).map_err(|e| e.to_string())?;
        }
    }
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiff::encoder::{colortype, Compression, TiffEncoder};

    fn pattern(w: u32, h: u32, c: u16) -> Vec<u16> {
        (0..h).flat_map(|y| (0..w).map(move |x| ((x * 7 + y * 13) % 4000) as u16 + c * 1000)).collect()
    }

    #[test]
    fn regions_and_shrinking_match_for_plain_and_compressed() {
        let dir = std::env::temp_dir().join(format!("fluora-tiffread-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (w, h) = (301u32, 207u32);
        let plain = dir.join("plain.tif");
        let lzw = dir.join("lzw.tif");
        {
            let mut e = TiffEncoder::new(File::create(&plain).unwrap()).unwrap();
            for c in 0..2 {
                let mut im = e.new_image::<colortype::Gray16>(w, h).unwrap();
                im.rows_per_strip(16).unwrap();
                im.write_data(&pattern(w, h, c)).unwrap();
            }
            let mut e = TiffEncoder::new(File::create(&lzw).unwrap()).unwrap().with_compression(Compression::Lzw);
            for c in 0..2 {
                let mut im = e.new_image::<colortype::Gray16>(w, h).unwrap();
                im.rows_per_strip(16).unwrap();
                im.write_data(&pattern(w, h, c)).unwrap();
            }
        }
        let full = pattern(w, h, 1);
        for p in [&plain, &lzw] {
            let a = read_window(p, None, 1).unwrap();
            assert_eq!(a.data.len(), 2);
            assert_eq!(a.data[1], full);
            // a crop
            let r = read_window(p, Some([0.25, 0.4, 0.3, 0.2]), 1).unwrap();
            let (x0, y0) = ((0.25 * w as f64).floor() as u32, (0.4 * h as f64).floor() as u32);
            assert_eq!(r.data[1][0], full[(y0 * w + x0) as usize]);
            assert_eq!(r.data[1][(r.w + 3) as usize], full[((y0 + 1) * w + x0 + 3) as usize]);
            // shrunk by 4: each pixel is the mean of its block
            let s = read_window(p, None, 4).unwrap();
            assert_eq!((s.w, s.h), (76, 52));
            let mean: u64 = (0..4).flat_map(|y| (0..4).map(move |x| (y, x))).map(|(y, x)| full[((4 + y) * w + 8 + x) as usize] as u64).sum::<u64>() / 16;
            assert_eq!(s.data[1][(1 * s.w + 2) as usize] as u64, mean);
        }
        // round trip through the cache format keeps names and pixel size
        let mut c = read_window(&plain, None, 2).unwrap();
        c.filters = vec!["DAPI".into(), "EGFP".into()];
        c.um = Some(0.65);
        let cache = dir.join("cache.tif");
        write_cache(&cache, &c).unwrap();
        let back = read_window(&cache, None, 1).unwrap();
        assert_eq!(back.filters, vec!["DAPI", "EGFP"]);
        assert!((back.um.unwrap() - 0.65).abs() < 1e-6);
        assert_eq!(back.data[1], c.data[1]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
