//! The puzzle file format: TOML wrapped around a grid drawn in text.
//!
//! ```toml
//! name = "Example"
//! grid = """
//! ABBC
//! ABBC
//! D..E
//! """
//!
//! [goal]
//! block = "B"
//! at = [1, 1]
//!
//! [blocks.DE]
//! tag = "small"
//! ```
//!
//! `at` is the column and row, counted from zero at the top left, where the
//! top-left corner of the block's bounding box must end up. An optional
//! top-level `exit = "down"` (or up, left, right) names the side the key block
//! leaves by once solved; it is worked out from the target when left out. Use `[[goal]]`
//! tables for several targets. A `[blocks.XYZ]` table applies to every block
//! named in its key.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

use crate::geometry::{Axis, Dir};
use crate::spec::{BlockAttrs, GoalSpec, PuzzleSpec, SpecError};

#[derive(Debug)]
pub enum FileError {
    Toml(toml::de::Error),
    /// A `block` value or `[blocks.*]` key that is not made of block labels.
    BadLabel(String),
    Spec(SpecError),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileError::Toml(e) => write!(f, "{e}"),
            FileError::BadLabel(l) => write!(f, "{l:?} is not a block label"),
            FileError::Spec(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FileError {}

impl From<SpecError> for FileError {
    fn from(e: SpecError) -> Self {
        FileError::Spec(e)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PuzzleFile {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    grid: String,
    #[serde(default)]
    goal: Goals,
    #[serde(default)]
    blocks: BTreeMap<String, BlockFile>,
    exit: Option<ExitFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExitFile {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Goals {
    One(GoalFile),
    Many(Vec<GoalFile>),
}

impl Default for Goals {
    fn default() -> Self {
        Goals::Many(Vec::new())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GoalFile {
    block: String,
    at: [u8; 2],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockFile {
    tag: Option<String>,
    axis: Option<Axis>,
}

/// Parses a puzzle file. The result still needs [`PuzzleSpec::build`].
pub fn parse(source: &str) -> Result<PuzzleSpec, FileError> {
    let file: PuzzleFile = toml::from_str(source).map_err(FileError::Toml)?;
    let mut spec = PuzzleSpec::from_grid(&file.grid)?;
    spec.name = file.name;
    spec.description = file.description;
    spec.exit = file.exit.map(|e| match e {
        ExitFile::Up => Dir::Up,
        ExitFile::Down => Dir::Down,
        ExitFile::Left => Dir::Left,
        ExitFile::Right => Dir::Right,
    });

    let goals = match file.goal {
        Goals::One(goal) => vec![goal],
        Goals::Many(goals) => goals,
    };
    for goal in goals {
        let mut chars = goal.block.chars();
        let block = match (chars.next(), chars.next()) {
            (Some(c), None) => c,
            _ => return Err(FileError::BadLabel(goal.block)),
        };
        spec.goals.push(GoalSpec { block, x: goal.at[0], y: goal.at[1] });
    }

    for (key, attrs) in file.blocks {
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(FileError::BadLabel(key));
        }
        for label in key.chars() {
            let entry = spec.blocks.entry(label).or_default();
            if attrs.tag.is_some() {
                entry.tag = attrs.tag.clone();
            }
            if let Some(axis) = attrs.axis {
                entry.axis = axis;
            }
        }
    }
    Ok(spec)
}

/// Writes a puzzle file that [`parse`] reads back to the same description.
pub fn write(spec: &PuzzleSpec) -> String {
    let quote = |s: &str| toml::Value::String(s.to_owned()).to_string();
    let mut out = String::new();
    if !spec.name.is_empty() {
        out.push_str(&format!("name = {}\n", quote(&spec.name)));
    }
    if !spec.description.is_empty() {
        out.push_str(&format!("description = {}\n", quote(&spec.description)));
    }
    out.push_str(&format!("grid = \"\"\"\n{}\"\"\"\n", spec.grid_string()));
    if let Some(dir) = spec.exit {
        let name = match dir {
            Dir::Up => "up",
            Dir::Down => "down",
            Dir::Left => "left",
            Dir::Right => "right",
        };
        out.push_str(&format!("exit = \"{name}\"\n"));
    }
    for goal in &spec.goals {
        let header = if spec.goals.len() == 1 { "[goal]" } else { "[[goal]]" };
        out.push_str(&format!(
            "\n{header}\nblock = \"{}\"\nat = [{}, {}]\n",
            goal.block, goal.x, goal.y
        ));
    }

    // Blocks with identical attributes share one table.
    let mut groups: Vec<(&BlockAttrs, String)> = Vec::new();
    for (label, attrs) in &spec.blocks {
        if *attrs == BlockAttrs::default() {
            continue;
        }
        match groups.iter_mut().find(|(a, _)| *a == attrs) {
            Some((_, labels)) => labels.push(*label),
            None => groups.push((attrs, label.to_string())),
        }
    }
    for (attrs, labels) in groups {
        out.push_str(&format!("\n[blocks.{labels}]\n"));
        if let Some(tag) = &attrs.tag {
            out.push_str(&format!("tag = {}\n", quote(tag)));
        }
        match attrs.axis {
            Axis::Free => {}
            Axis::Horizontal => out.push_str("axis = \"horizontal\"\n"),
            Axis::Vertical => out.push_str("axis = \"vertical\"\n"),
        }
    }
    out
}
