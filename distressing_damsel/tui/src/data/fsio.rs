//! Small file helpers.

use std::io;
use std::path::Path;

/// Writes a file in one step: the contents go to a temporary file beside
/// it, which then replaces the target, so a crash never leaves half a file.
/// Creates missing parent folders.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Reads a file, treating a missing one as `None`.
pub fn read_if_exists(path: &Path) -> io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::home::testing::TempDir;

    #[test]
    fn writes_create_folders_and_replace_whole_files() {
        let tmp = TempDir::new();
        let path = tmp.0.join("a/b/file.toml");
        assert_eq!(read_if_exists(&path).unwrap(), None);
        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();
        assert_eq!(read_if_exists(&path).unwrap().as_deref(), Some("second"));
        // No temporary files left behind.
        let names: Vec<_> = std::fs::read_dir(tmp.0.join("a/b")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["file.toml"]);
    }
}
