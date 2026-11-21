//! Where the data folder is.

use std::path::{Path, PathBuf};

/// The data folder. Settings live directly in it; everything else lives in
/// the data directory, which is this folder unless `data_dir` says
/// otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    /// `$DD_HOME` if set, otherwise `.data/` in the workspace this program
    /// was built from.
    pub fn from_env() -> Home {
        match std::env::var_os("DD_HOME").filter(|v| !v.is_empty()) {
            Some(path) => Home::at(PathBuf::from(path)),
            None => Home::at(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".data")),
        }
    }

    pub fn at(root: impl Into<PathBuf>) -> Home {
        let root: PathBuf = root.into();
        // Tidy `tui/../.data` into `.data` for display.
        let root = std::fs::canonicalize(root.parent().unwrap_or(Path::new(".")))
            .map(|p| p.join(root.file_name().unwrap_or_default()))
            .unwrap_or(root);
        Home { root }
    }

    #[cfg_attr(not(test), allow(dead_code))] // used by tests and the data-folder move
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.toml")
    }

    /// Where puzzles, progress and records go: `data_dir` if given (a
    /// relative path counts from this folder), otherwise this folder.
    pub fn data_dir(&self, data_dir: Option<&Path>) -> PathBuf {
        match data_dir {
            Some(dir) if dir.is_absolute() => dir.to_path_buf(),
            Some(dir) => self.root.join(dir),
            None => self.root.clone(),
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh, empty folder under the system temporary directory, removed
    /// when dropped.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new() -> TempDir {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("dd-test-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::TempDir;
    use super::*;

    #[test]
    fn default_is_beside_the_workspace() {
        let home = Home::at(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".data"));
        assert!(home.root().ends_with("distressing_damsel/.data"), "{}", home.root().display());
        assert!(!home.root().to_string_lossy().contains(".."));
    }

    #[test]
    fn data_dir_resolves_against_the_folder() {
        let tmp = TempDir::new();
        let home = Home::at(&tmp.0);
        assert_eq!(home.settings_file(), home.root().join("settings.toml"));
        assert_eq!(home.data_dir(None), home.root());
        assert_eq!(home.data_dir(Some(Path::new("elsewhere"))), home.root().join("elsewhere"));
        assert_eq!(home.data_dir(Some(Path::new("/abs/dir"))), PathBuf::from("/abs/dir"));
    }
}
