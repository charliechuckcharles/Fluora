use fluora_naming::{detect, rename, Kind};
use std::fs::{self, File};
use std::path::Path;
use tiff::encoder::{colortype, TiffEncoder};

fn grey(path: &Path, w: u32, h: u32, channels: u32) {
    let mut enc = TiffEncoder::new(File::create(path).unwrap()).unwrap();
    for c in 0..channels {
        let px: Vec<u16> = (0..w * h).map(|i| (i % 65535) as u16 + c as u16).collect();
        enc.write_image::<colortype::Gray16>(w, h, &px).unwrap();
    }
}
fn rgb(path: &Path, w: u32, h: u32) {
    let mut enc = TiffEncoder::new(File::create(path).unwrap()).unwrap();
    enc.write_image::<colortype::RGB8>(w, h, &vec![100u8; (w * h * 3) as usize]).unwrap();
}

#[test]
fn finds_pyramids_singles_and_specials_then_renames_and_undoes() {
    let root = std::env::temp_dir().join(format!("fluora-naming-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let slide = root.join("36");
    fs::create_dir_all(&slide).unwrap();
    // pyramid of 3, pyramid of 2, a single-size image, then label + overview
    for (i, (w, h)) in [(400, 300), (200, 150), (100, 75), (500, 320), (250, 160), (90, 90)].iter().enumerate() {
        grey(&slide.join(format!("scan_series{i}.tif")), *w, *h, 2);
    }
    rgb(&slide.join("scan_series6.tif"), 60, 30);
    rgb(&slide.join("scan_series7.tif"), 120, 50);
    fs::write(slide.join("scan.czi"), b"x").unwrap();
    // a folder already named by the convention, split by stain
    let other = root.join("micro");
    fs::create_dir_all(&other).unwrap();
    grey(&other.join("03p_Cad_SL47_IM01_BF-DAPI_G[BF]_20x_HD_20260419_CNH.tif"), 30, 20, 1);
    grey(&other.join("03p_Cad_SL47_IM01_BF-DAPI_G[DAPI]_20x_HD_20260419_CNH.tif"), 30, 20, 1);
    // a CZI with nothing unpacked
    let packed = root.join("packed");
    fs::create_dir_all(&packed).unwrap();
    fs::write(packed.join("p.czi"), b"x").unwrap();

    let mut det = detect(&root, "CNH", &|_| {});
    assert_eq!(det.packed.len(), 1);
    assert_eq!(det.groups.len(), 2);
    let g = &det.groups[0];
    assert_eq!(g.rel, "36");
    let kinds: Vec<(Kind, usize)> = g.entries.iter().map(|e| (e.kind, e.parts.len())).collect();
    assert_eq!(kinds, vec![(Kind::Image, 3), (Kind::Image, 2), (Kind::Image, 1), (Kind::Label, 1), (Kind::Overview, 1)]);
    assert_eq!(g.shared.slide, "36");
    assert_eq!(g.shared.initials, "CNH");
    assert_eq!(g.entries[2].fields.im, "03");
    let m = &det.groups[1];
    assert!(m.entries[0].split && m.entries[0].named);
    assert_eq!(m.shared.cond, "Cad");
    assert_eq!(m.filters, vec!["BF", "DAPI"]);

    // missing values block renaming
    let p = rename::plan(&det);
    assert!(p.blocking() > 0);
    assert!(rename::execute(&p, "t1", &|_, _| {}).is_err());

    // fill them in
    let g = &mut det.groups[0];
    g.shared.cond = "E".into();
    g.shared.stains = "GAF-DAPI".into();
    g.shared.obj = "20x".into();
    g.shared.sample = "01o".into();
    let p = rename::plan(&det);
    assert_eq!(p.blocking(), 0, "{:?}", p.problems);
    assert_eq!(p.names[0][0][1], format!("01o_E_SL36_IM01_GAF-DAPI_G[R2]_20x_{}_CNH.tif", det.groups[0].shared.date));
    assert_eq!(p.names[0][2][0], format!("01o_E_SL36_IM03_GAF-DAPI_20x_{}_CNH.tif", det.groups[0].shared.date));
    assert!(p.names[0][3][0].contains("_SL36_SlideLabel_"));
    assert_eq!(p.czi_names[0].as_deref(), Some(&*format!("01o_E_SL36_GAF-DAPI_20x_{}_CNH.czi", det.groups[0].shared.date)));
    assert_eq!(p.renames.len(), 9); // 8 TIFFs + the CZI; the named folder is unchanged

    // duplicate image numbers are caught
    det.groups[0].entries[1].fields.im = "01".into();
    assert!(rename::plan(&det).problems.iter().any(|x| x.kind == "duplicate"));
    det.groups[0].entries[1].fields.im = "02".into();

    let p = rename::plan(&det);
    let out = rename::execute(&p, "t2", &|_, _| {}).unwrap();
    assert_eq!(out.renamed, 9);
    assert!(out.failed.is_empty());
    rename::apply(&mut det, &p);
    assert!(det.groups[0].entries.iter().all(|e| e.named));
    // a fresh look reads everything from the new names
    let again = detect(&root, "", &|_| {});
    assert_eq!(again.groups[0].entries.len(), 5);
    assert!(again.groups[0].entries.iter().all(|e| e.named));
    assert_eq!(again.groups[0].shared.cond, "E");

    assert_eq!(rename::history(&slide).len(), 1);
    let u = rename::undo(&slide, "t2");
    assert_eq!(u.renamed, 9);
    assert!(slide.join("scan_series0.tif").exists() && slide.join("scan.czi").exists());
    assert!(rename::history(&slide).is_empty());
    fs::remove_dir_all(&root).unwrap();
}
