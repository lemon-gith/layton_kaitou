//! Everything `dd` keeps on disk: settings, your puzzles, progress on each
//! puzzle and your best results.
//!
//! It all lives under one folder, [`Home`], normally `.data/` next to the
//! workspace (git-ignored). Settings always stay there; puzzles, progress
//! and records can be moved to another folder with the `data_dir` setting.

pub mod fsio;
pub mod home;
// Used by the play screen.
#[allow(dead_code)]
pub mod progress;
#[allow(dead_code)]
pub mod records;

pub use home::Home;
