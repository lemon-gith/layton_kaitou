//! Moving puzzles, progress and records to another folder.

use std::path::{Path, PathBuf};

/// What moving the data did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Files copied to the new folder.
    pub copied: usize,
    /// Files already in the new folder, left as they were.
    pub skipped: Vec<PathBuf>,
}

/// The files and folders that make up the data, relative to the data
/// folder. Settings are not among them: they stay in the home folder.
const ENTRIES: [&str; 4] = ["puzzles", "progress", "records.toml", "session.toml"];

/// Copies the data from `from` to `to`. Nothing is deleted or replaced:
/// files already in `to` are skipped and reported, and the old copy stays
/// for the player to remove.
pub fn copy_data(from: &Path, to: &Path) -> Result<Report, String> {
    let mut report = Report::default();
    if same_place(from, to) {
        return Ok(report);
    }
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in ENTRIES {
        copy_tree(&from.join(entry), &to.join(entry), &mut report)?;
    }
    Ok(report)
}

fn same_place(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn copy_tree(from: &Path, to: &Path, report: &mut Report) -> Result<(), String> {
    let err = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    let Ok(meta) = std::fs::metadata(from) else {
        return Ok(());
    };
    if meta.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| err(to, e))?;
        for entry in std::fs::read_dir(from).map_err(|e| err(from, e))? {
            let entry = entry.map_err(|e| err(from, e))?;
            copy_tree(&entry.path(), &to.join(entry.file_name()), report)?;
        }
    } else if to.exists() {
        report.skipped.push(to.to_path_buf());
    } else {
        std::fs::copy(from, to).map_err(|e| err(to, e))?;
        report.copied += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::home::testing::TempDir;

    #[test]
    fn copies_everything_but_settings_and_replaces_nothing() {
        let tmp = TempDir::new();
        let (old, new) = (tmp.0.join("old"), tmp.0.join("new"));
        for (path, text) in [
            ("settings.toml", "theme = \"dark\""),
            ("records.toml", "version = 1"),
            ("puzzles/a.toml", "a"),
            ("progress/1.toml", "one"),
            ("progress/2.toml", "two"),
        ] {
            let p = old.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        // Already in the new place: must survive untouched.
        std::fs::create_dir_all(new.join("progress")).unwrap();
        std::fs::write(new.join("progress/2.toml"), "theirs").unwrap();

        let report = copy_data(&old, &new).unwrap();
        assert_eq!(report.copied, 3);
        assert_eq!(report.skipped, [new.join("progress/2.toml")]);
        assert_eq!(std::fs::read_to_string(new.join("progress/2.toml")).unwrap(), "theirs");
        assert_eq!(std::fs::read_to_string(new.join("puzzles/a.toml")).unwrap(), "a");
        assert!(!new.join("settings.toml").exists());
        // The old copy is untouched.
        assert_eq!(std::fs::read_to_string(old.join("progress/2.toml")).unwrap(), "two");

        // Moving onto itself does nothing.
        assert_eq!(copy_data(&new, &new).unwrap(), Report::default());
    }
}
