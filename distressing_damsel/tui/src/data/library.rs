//! Your puzzles folder, and which puzzle you played last.

use std::path::{Path, PathBuf};

use dd_engine::{file, PuzzleSpec};
use serde::{Deserialize, Serialize};

use super::fsio;

/// Where a puzzle came from, so Continue can find it again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Source {
    BuiltIn { name: String },
    /// A file in the puzzles folder, by file name.
    File { file: String },
    /// Opened from a pasted share code and not saved.
    Code,
}

/// The puzzle Continue reopens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastPuzzle {
    pub name: String,
    /// Share code: enough to rebuild the puzzle if its file has gone.
    pub code: String,
    pub source: Source,
}

#[derive(Default, Serialize, Deserialize)]
struct SessionFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    last: Option<LastPuzzle>,
}

pub fn puzzles_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("puzzles")
}

pub fn load_last(data_dir: &Path) -> Result<Option<LastPuzzle>, String> {
    let path = data_dir.join("session.toml");
    let shown = path.display();
    match fsio::read_if_exists(&path).map_err(|e| format!("{shown}: {e}"))? {
        None => Ok(None),
        Some(text) => {
            let file: SessionFile = toml::from_str(&text).map_err(|e| format!("{shown}: {}", e.message()))?;
            Ok(file.last)
        }
    }
}

pub fn save_last(data_dir: &Path, last: &LastPuzzle) -> Result<(), String> {
    let path = data_dir.join("session.toml");
    let text = toml::to_string(&SessionFile { last: Some(last.clone()) }).map_err(|e| e.to_string())?;
    fsio::write_atomic(&path, &text).map_err(|e| format!("{}: {e}", path.display()))
}

/// One file in the puzzles folder: the puzzle, or why it could not be read.
#[derive(Debug)]
pub struct Listed {
    pub file: String,
    pub puzzle: Result<PuzzleSpec, String>,
}

/// The `.toml` files in the puzzles folder, by file name. A missing folder
/// is an empty list.
pub fn list_puzzles(data_dir: &Path) -> Result<Vec<Listed>, String> {
    let dir = puzzles_dir(data_dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut listed: Vec<Listed> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml") && p.is_file())
        .map(|path| {
            let file = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let puzzle = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| file::parse(&text).map_err(|e| e.to_string()));
            Listed { file, puzzle }
        })
        .collect();
    listed.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(listed)
}

pub fn read_puzzle(data_dir: &Path, file: &str) -> Result<PuzzleSpec, String> {
    let path = puzzles_dir(data_dir).join(file);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    file::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Saves a puzzle into the puzzles folder under a file name made from its
/// name, never replacing an existing file. Returns the file name.
pub fn save_puzzle(data_dir: &Path, spec: &PuzzleSpec) -> Result<String, String> {
    let dir = puzzles_dir(data_dir);
    let mut stem: String = spec
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if stem.is_empty() {
        stem = "puzzle".into();
    }
    let name = (1..)
        .map(|n| if n == 1 { format!("{stem}.toml") } else { format!("{stem}_{n}.toml") })
        .find(|name| !dir.join(name).exists())
        .expect("some name is free");
    fsio::write_atomic(&dir.join(&name), &file::write(spec)).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::home::testing::TempDir;

    fn spec(name: &str) -> PuzzleSpec {
        let mut spec = file::parse(include_str!("../../../puzzles/first_steps.toml")).unwrap();
        spec.name = name.into();
        spec
    }

    #[test]
    fn last_puzzle_round_trips() {
        let tmp = TempDir::new();
        assert_eq!(load_last(&tmp.0).unwrap(), None);
        for source in [
            Source::BuiltIn { name: "klotski".into() },
            Source::File { file: "mine.toml".into() },
            Source::Code,
        ] {
            let last = LastPuzzle { name: "Mine".into(), code: "dd1-abc".into(), source };
            save_last(&tmp.0, &last).unwrap();
            assert_eq!(load_last(&tmp.0).unwrap(), Some(last));
        }
    }

    #[test]
    fn saving_never_replaces_a_puzzle() {
        let tmp = TempDir::new();
        assert!(list_puzzles(&tmp.0).unwrap().is_empty());
        assert_eq!(save_puzzle(&tmp.0, &spec("My Puzzle!")).unwrap(), "my_puzzle.toml");
        assert_eq!(save_puzzle(&tmp.0, &spec("my puzzle")).unwrap(), "my_puzzle_2.toml");
        assert_eq!(save_puzzle(&tmp.0, &spec("")).unwrap(), "puzzle.toml");
        let read = read_puzzle(&tmp.0, "my_puzzle.toml").unwrap();
        assert_eq!(read.cells, spec("x").cells);
        assert_eq!(read.name, "My Puzzle!");
    }

    #[test]
    fn listing_reports_unreadable_files_without_failing() {
        let tmp = TempDir::new();
        save_puzzle(&tmp.0, &spec("Good")).unwrap();
        let dir = puzzles_dir(&tmp.0);
        std::fs::write(dir.join("broken.toml"), "grid = 3").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let listed = list_puzzles(&tmp.0).unwrap();
        let names: Vec<&str> = listed.iter().map(|l| l.file.as_str()).collect();
        assert_eq!(names, ["broken.toml", "good.toml"]);
        assert!(listed[0].puzzle.is_err());
        assert!(listed[1].puzzle.is_ok());
    }
}
