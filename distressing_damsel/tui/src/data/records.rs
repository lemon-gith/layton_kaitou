//! Your best results on each puzzle, in `records.toml`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::fsio;
use super::progress::Counters;
use crate::settings::BestShown;

const VERSION: u32 = 1;

/// One solve: how many moves, and what help it had.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub moves: u32,
    #[serde(default)]
    pub hints: u32,
    #[serde(default)]
    pub solution_viewed: bool,
}

impl Score {
    pub fn new(moves: usize, counters: Counters) -> Score {
        Score { moves: moves as u32, hints: counters.hints, solution_viewed: counters.solution_viewed }
    }

    /// Solved with no hints and without viewing the solution.
    pub fn unaided(&self) -> bool {
        self.hints == 0 && !self.solution_viewed
    }

    /// Fewer moves wins; on a tie, fewer hints, then not having viewed the
    /// solution.
    fn better_than(&self, other: &Score) -> bool {
        (self.moves, self.hints, self.solution_viewed) < (other.moves, other.hints, other.solution_viewed)
    }
}

/// `85`, `85 h:2`, `85 s` or `85 h:2 s`.
impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.moves)?;
        if self.hints > 0 {
            write!(f, " h:{}", self.hints)?;
        }
        if self.solution_viewed {
            write!(f, " s")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    /// For people reading the file.
    pub name: String,
    pub solves: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best: Option<Score>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_unaided: Option<Score>,
}

impl Record {
    /// The best to show, by the `best` setting.
    pub fn shown(&self, choice: BestShown) -> Option<Score> {
        match choice {
            BestShown::Overall => self.best,
            BestShown::NoHintsFirst => self.best_unaided.or(self.best),
        }
    }
}

/// What a new solve changed.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Improvement {
    pub best: bool,
    pub best_unaided: bool,
    /// Whether the best the player has chosen to see is now this solve.
    pub shown: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Records {
    /// By puzzle id.
    pub puzzles: BTreeMap<String, Record>,
}

#[derive(Serialize, Deserialize)]
struct RecordsFile {
    version: u32,
    #[serde(default)]
    puzzles: BTreeMap<String, Record>,
}

impl Records {
    pub fn get(&self, id: &str) -> Option<&Record> {
        self.puzzles.get(id)
    }

    /// Records a solve and says which bests it improved. `choice` is the
    /// `best` setting, for [`Improvement::shown`].
    pub fn add_solve(&mut self, id: &str, name: &str, score: Score, choice: BestShown) -> Improvement {
        let record = self.puzzles.entry(id.to_string()).or_default();
        record.name = name.to_string();
        record.solves += 1;
        let before = record.shown(choice);
        let mut improvement = Improvement::default();
        if record.best.is_none_or(|b| score.better_than(&b)) {
            record.best = Some(score);
            improvement.best = true;
        }
        if score.unaided() && record.best_unaided.is_none_or(|b| score.better_than(&b)) {
            record.best_unaided = Some(score);
            improvement.best_unaided = true;
        }
        improvement.shown = record.shown(choice) != before;
        improvement
    }

    pub fn load(data_dir: &Path) -> Result<Records, String> {
        let path = data_dir.join("records.toml");
        let shown = path.display();
        let Some(text) = fsio::read_optional(&path).map_err(|e| format!("{shown}: {e}"))? else {
            return Ok(Records::default());
        };
        let file: RecordsFile = toml::from_str(&text).map_err(|e| format!("{shown}: {}", e.message()))?;
        if file.version != VERSION {
            return Err(format!("{shown}: saved by a different version of dd (format {})", file.version));
        }
        Ok(Records { puzzles: file.puzzles })
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        let path = data_dir.join("records.toml");
        let file = RecordsFile { version: VERSION, puzzles: self.puzzles.clone() };
        let text = toml::to_string(&file).map_err(|e| e.to_string())?;
        fsio::write_atomic(&path, &text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::home::testing::TempDir;

    fn score(moves: u32, hints: u32, solution_viewed: bool) -> Score {
        Score { moves, hints, solution_viewed }
    }

    #[test]
    fn scores_display_compactly() {
        assert_eq!(score(85, 0, false).to_string(), "85");
        assert_eq!(score(85, 2, false).to_string(), "85 h:2");
        assert_eq!(score(85, 0, true).to_string(), "85 s");
        assert_eq!(score(85, 2, true).to_string(), "85 h:2 s");
    }

    #[test]
    fn bests_track_overall_and_unaided_separately() {
        let mut records = Records::default();
        let overall = BestShown::Overall;
        // An aided solve sets the overall best only.
        let first = records.add_solve("k", "Klotski", score(90, 2, false), overall);
        assert_eq!(first, Improvement { best: true, best_unaided: false, shown: true });
        // A longer unaided solve: unaided best only, which "overall" ignores.
        let second = records.add_solve("k", "Klotski", score(95, 0, false), overall);
        assert_eq!(second, Improvement { best: false, best_unaided: true, shown: false });
        // A worse aided solve changes nothing.
        assert_eq!(records.add_solve("k", "Klotski", score(99, 1, false), overall), Improvement::default());
        // Same moves, fewer hints, is better.
        assert!(records.add_solve("k", "Klotski", score(90, 1, false), overall).best);

        let r = records.get("k").unwrap();
        assert_eq!(r.solves, 4);
        assert_eq!(r.shown(BestShown::Overall), Some(score(90, 1, false)));
        assert_eq!(r.shown(BestShown::NoHintsFirst), Some(score(95, 0, false)));
    }

    #[test]
    fn no_hints_first_falls_back_to_the_overall_best() {
        let mut records = Records::default();
        records.add_solve("k", "Klotski", score(90, 0, true), BestShown::NoHintsFirst);
        let r = records.get("k").unwrap();
        assert_eq!(r.best_unaided, None);
        assert_eq!(r.shown(BestShown::NoHintsFirst), Some(score(90, 0, true)));
    }

    #[test]
    fn new_best_follows_what_is_shown() {
        let choice = BestShown::NoHintsFirst;
        let mut records = Records::default();
        // No unaided best yet: an aided solve is what is shown, so it is new.
        assert!(records.add_solve("k", "K", score(90, 2, false), choice).shown);
        // A better aided solve still is, while there is no unaided best.
        assert!(records.add_solve("k", "K", score(85, 1, false), choice).shown);
        // The first unaided solve takes over what is shown.
        assert!(records.add_solve("k", "K", score(99, 0, false), choice).shown);
        // From now on aided solves never change what is shown.
        let aided = records.add_solve("k", "K", score(70, 1, false), choice);
        assert!(aided.best && !aided.shown);
        assert!(records.add_solve("k", "K", score(95, 0, false), choice).shown);
    }

    #[test]
    fn records_round_trip() {
        let tmp = TempDir::new();
        assert_eq!(Records::load(&tmp.0).unwrap(), Records::default());
        let mut records = Records::default();
        records.add_solve("abc", "Klotski", score(81, 0, false), BestShown::Overall);
        records.add_solve("def", "Gridlock", score(12, 3, true), BestShown::Overall);
        records.save(&tmp.0).unwrap();
        assert_eq!(Records::load(&tmp.0).unwrap(), records);
        let text = std::fs::read_to_string(tmp.0.join("records.toml")).unwrap();
        assert!(text.contains("[puzzles.abc]") && text.contains("Klotski"), "{text}");
    }
}
