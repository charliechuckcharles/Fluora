//! The lab's microscopy file-naming rules, shared by Fluora and the standalone renamer.
pub mod convention;
pub mod czi;
pub mod detect;
pub mod rename;
pub mod tasks;
pub mod tiffinfo;

pub use convention::{Fields, Special};
pub use detect::{detect, merge, Detection, Entry, Group, Kind};
