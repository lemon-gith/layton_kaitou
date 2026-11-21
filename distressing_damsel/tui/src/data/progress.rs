//! Progress on a puzzle: the current moves, the attempt's counters and its
//! checkpoints, saved in `progress/<id>.toml`.

use std::path::{Path, PathBuf};

use dd_engine::{Drag, Game, MoveRecord, Puzzle};
use serde::{Deserialize, Serialize};

use super::fsio;

const VERSION: u32 = 1;

/// A short id for a puzzle, from its share code: the same puzzle always
/// gets the same id, whatever its file is called. FNV-1a, 64 bits, in hex.
pub fn puzzle_id(code: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in code.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// What an attempt has used. Both only go up until the puzzle is restarted.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Counters {
    pub hints: u32,
    pub solution_viewed: bool,
}

impl Counters {
    /// The larger of each, for opening a checkpoint without undoing hints.
    pub fn max(self, other: Counters) -> Counters {
        Counters {
            hints: self.hints.max(other.hints),
            solution_viewed: self.solution_viewed || other.solution_viewed,
        }
    }
}

/// A saved route the player can return to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    /// Shown as ⚑ 3. Numbers are never reused within a puzzle.
    pub number: u32,
    pub history: Vec<Drag>,
    pub counters: Counters,
}

/// Everything saved about one puzzle in progress.
#[derive(Clone, Debug)]
pub struct Progress {
    pub game: Game,
    pub counters: Counters,
    pub checkpoints: Vec<Checkpoint>,
    pub next_checkpoint: u32,
}

impl Progress {
    pub fn new(puzzle: Puzzle) -> Progress {
        Progress { game: Game::new(puzzle), counters: Counters::default(), checkpoints: Vec::new(), next_checkpoint: 1 }
    }
}

/// The file's layout.
#[derive(Serialize, Deserialize)]
struct ProgressFile {
    version: u32,
    /// For people reading the file; not used to find the puzzle.
    name: String,
    /// The puzzle's share code, checked on load.
    code: String,
    counters: Counters,
    moves: Vec<MoveRecord>,
    next_checkpoint: u32,
    #[serde(default)]
    checkpoints: Vec<CheckpointFile>,
}

#[derive(Serialize, Deserialize)]
struct CheckpointFile {
    number: u32,
    counters: Counters,
    moves: Vec<MoveRecord>,
}

pub fn progress_file(data_dir: &Path, code: &str) -> PathBuf {
    data_dir.join("progress").join(format!("{}.toml", puzzle_id(code)))
}

/// Saves progress on the puzzle whose share code is `code`.
pub fn save(data_dir: &Path, code: &str, progress: &Progress) -> Result<(), String> {
    let game = &progress.game;
    let records = |history: &[Drag]| {
        Game::from_history(game.puzzle().clone(), history).map(|g| g.records()).map_err(|e| e.to_string())
    };
    let file = ProgressFile {
        version: VERSION,
        name: game.puzzle().name.clone(),
        code: code.to_string(),
        counters: progress.counters,
        moves: game.records(),
        next_checkpoint: progress.next_checkpoint,
        checkpoints: progress
            .checkpoints
            .iter()
            .map(|c| Ok(CheckpointFile { number: c.number, counters: c.counters, moves: records(&c.history)? }))
            .collect::<Result<_, String>>()?,
    };
    let path = progress_file(data_dir, code);
    let text = toml::to_string(&file).map_err(|e| e.to_string())?;
    fsio::write_atomic(&path, &text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Loads progress on `puzzle`, whose share code is `code`. `None` if there
/// is none. A file that does not match the puzzle, or whose moves do not
/// replay, is an error; it is left in place for the player to look at.
pub fn load(data_dir: &Path, code: &str, puzzle: &Puzzle) -> Result<Option<Progress>, String> {
    let path = progress_file(data_dir, code);
    let shown = path.display();
    let Some(text) = fsio::read_optional(&path).map_err(|e| format!("{shown}: {e}"))? else {
        return Ok(None);
    };
    let file: ProgressFile = toml::from_str(&text).map_err(|e| format!("{shown}: {}", e.message()))?;
    if file.version != VERSION {
        return Err(format!("{shown}: saved by a different version of dd (format {})", file.version));
    }
    if file.code != code {
        return Err(format!("{shown}: belongs to a different puzzle"));
    }
    let game = Game::from_records(puzzle.clone(), &file.moves).map_err(|e| format!("{shown}: {e}"))?;
    let checkpoints = file
        .checkpoints
        .iter()
        .map(|c| {
            let history = Game::drags_from_records(puzzle, &c.moves)
                .and_then(|h| Game::from_history(puzzle.clone(), &h).map(|_| h))
                .map_err(|e| format!("{shown}: checkpoint {}: {e}", c.number))?;
            Ok(Checkpoint { number: c.number, history, counters: c.counters })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let highest = checkpoints.iter().map(|c| c.number).max().unwrap_or(0);
    Ok(Some(Progress {
        game,
        counters: file.counters,
        checkpoints,
        next_checkpoint: file.next_checkpoint.max(highest + 1),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::home::testing::TempDir;
    use dd_engine::{file, Dir, PuzzleSpec};

    fn klotski() -> (PuzzleSpec, Puzzle, String) {
        let spec = file::parse(include_str!("../../../puzzles/klotski.toml")).unwrap();
        let puzzle = spec.build().unwrap().0;
        let code = spec.to_code().unwrap();
        (spec, puzzle, code)
    }

    fn block(game: &Game, label: char) -> usize {
        game.puzzle().blocks.iter().position(|b| b.label == label).unwrap()
    }

    #[test]
    fn ids_are_stable_and_short() {
        // Known FNV-1a values, so ids never change between versions.
        assert_eq!(puzzle_id(""), "cbf29ce484222325");
        assert_eq!(puzzle_id("a"), "af63dc4c8601ec8c");
        let (_, _, code) = klotski();
        assert_eq!(puzzle_id(&code), puzzle_id(&code.clone()));
        assert_eq!(puzzle_id(&code).len(), 16);
    }

    #[test]
    fn nothing_saved_loads_as_none() {
        let tmp = TempDir::new();
        let (_, puzzle, code) = klotski();
        assert!(load(&tmp.0, &code, &puzzle).unwrap().is_none());
    }

    #[test]
    fn progress_round_trips_with_checkpoints() {
        let tmp = TempDir::new();
        let (_, puzzle, code) = klotski();
        let mut progress = Progress::new(puzzle.clone());
        let g = block(&progress.game, 'G');
        progress.game.step(g, Dir::Down).unwrap();
        let first = progress.game.history().to_vec();
        let j = block(&progress.game, 'J');
        progress.game.step(j, Dir::Left).unwrap();
        progress.counters = Counters { hints: 2, solution_viewed: false };
        progress.checkpoints.push(Checkpoint { number: 1, history: first.clone(), counters: Counters::default() });
        progress.checkpoints.push(Checkpoint {
            number: 3,
            history: progress.game.history().to_vec(),
            counters: progress.counters,
        });
        progress.next_checkpoint = 4;
        save(&tmp.0, &code, &progress).unwrap();

        let path = progress_file(&tmp.0, &code);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("name = \"Klotski\""), "{text}");

        let back = load(&tmp.0, &code, &puzzle).unwrap().unwrap();
        assert_eq!(back.game.history(), progress.game.history());
        assert_eq!(back.game.position(), progress.game.position());
        assert_eq!(back.counters, progress.counters);
        assert_eq!(back.checkpoints, progress.checkpoints);
        assert_eq!(back.next_checkpoint, 4);
    }

    #[test]
    fn mismatched_or_damaged_files_are_errors_and_left_alone() {
        let tmp = TempDir::new();
        let (_, puzzle, code) = klotski();
        let path = progress_file(&tmp.0, &code);
        let mut progress = Progress::new(puzzle.clone());
        let g = block(&progress.game, 'G');
        progress.game.step(g, Dir::Down).unwrap();
        save(&tmp.0, &code, &progress).unwrap();
        let good = std::fs::read_to_string(&path).unwrap();

        // An illegal move.
        std::fs::write(&path, good.replace("to = [1, 4]", "to = [1, 0]")).unwrap();
        assert!(load(&tmp.0, &code, &puzzle).unwrap_err().contains("not possible"));
        assert!(path.exists());

        // Someone else's puzzle under this name.
        std::fs::write(&path, good.replace(&code, "dd1-other")).unwrap();
        assert!(load(&tmp.0, &code, &puzzle).unwrap_err().contains("different puzzle"));

        std::fs::write(&path, good.replace("version = 1", "version = 99")).unwrap();
        assert!(load(&tmp.0, &code, &puzzle).unwrap_err().contains("different version"));

        std::fs::write(&path, "not toml [").unwrap();
        assert!(load(&tmp.0, &code, &puzzle).is_err());
    }

    #[test]
    fn checkpoint_numbers_are_never_reused() {
        let tmp = TempDir::new();
        let (_, puzzle, code) = klotski();
        let mut progress = Progress::new(puzzle.clone());
        progress.checkpoints.push(Checkpoint { number: 7, history: Vec::new(), counters: Counters::default() });
        progress.next_checkpoint = 2; // out of step, e.g. edited by hand
        save(&tmp.0, &code, &progress).unwrap();
        assert_eq!(load(&tmp.0, &code, &puzzle).unwrap().unwrap().next_checkpoint, 8);
    }

    #[test]
    fn counters_combine_upwards() {
        let a = Counters { hints: 3, solution_viewed: false };
        let b = Counters { hints: 1, solution_viewed: true };
        assert_eq!(a.max(b), Counters { hints: 3, solution_viewed: true });
    }
}
