//! Rules engine for sliding-block puzzles.
//!
//! The crate is layered so that each consumer takes only what it needs:
//!
//! * [`Layout`] is the immutable part of a puzzle: board, block classes, goal.
//! * [`Position`] is a small copyable value saying where every block is.
//!   Blocks of the same class are interchangeable in it.
//! * Move generation is a set of pure functions on those two.
//! * [`Game`] wraps them for interactive play: block identities, a coalesced
//!   move history, undo and redo.
//!
//! Nothing here does I/O, spawns threads or reads the clock.

mod bits;
pub mod code;
pub mod file;
pub mod game;
pub mod geometry;
pub mod layout;
pub mod position;
pub mod spec;

pub use game::{BlockId, Drag, Game, HistoryError, MoveError, MoveRecord};
pub use geometry::{bit, cells, Axis, Cell, Dir, Mask, Transform, MAX_CELLS, NO_CELL};
pub use layout::{Class, ClassDef, ClassId, Goal, Layout, LayoutError, Shape, Symmetry};
pub use position::{Metric, Move, Position};
pub use spec::{BlockAttrs, BlockInfo, CellSpec, GoalSpec, Puzzle, PuzzleSpec, SpecError, Warning};
