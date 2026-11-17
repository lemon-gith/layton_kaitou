//! Interactive play: block identities, coalesced history, undo and redo.

use std::fmt;

use crate::geometry::{bit, Cell, Dir, Mask};
use crate::layout::Layout;
use crate::position::{Move, Position};
use crate::spec::Puzzle;

/// Index into [`Puzzle::blocks`].
pub type BlockId = usize;

/// One counted move: a block taken from one anchor to another, however many
/// steps it took to get there.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Drag {
    pub block: BlockId,
    pub from: Cell,
    pub to: Cell,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MoveError {
    NoSuchBlock,
    /// Something is in the way, or the block may not move like that.
    Blocked,
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveError::NoSuchBlock => write!(f, "no such block"),
            MoveError::Blocked => write!(f, "the block cannot move there"),
        }
    }
}

impl std::error::Error for MoveError {}

/// A puzzle being played.
///
/// The move count follows the drag rule: consecutive steps of one block are a
/// single move, and a block put back where its move began is no move at all.
#[derive(Clone, Debug)]
pub struct Game {
    puzzle: Puzzle,
    anchors: Vec<Cell>,
    position: Position,
    history: Vec<Drag>,
    undone: Vec<Drag>,
}

impl Game {
    pub fn new(puzzle: Puzzle) -> Game {
        let anchors = puzzle.blocks.iter().map(|b| b.start).collect();
        let position = puzzle.start;
        Game { puzzle, anchors, position, history: Vec::new(), undone: Vec::new() }
    }

    pub fn puzzle(&self) -> &Puzzle {
        &self.puzzle
    }

    pub fn layout(&self) -> &Layout {
        &self.puzzle.layout
    }

    pub fn position(&self) -> &Position {
        &self.position
    }

    pub fn block_count(&self) -> usize {
        self.anchors.len()
    }

    /// Current anchor of a block.
    pub fn anchor(&self, block: BlockId) -> Option<Cell> {
        self.anchors.get(block).copied()
    }

    /// Cells a block covers now.
    pub fn block_cells(&self, block: BlockId) -> Mask {
        match self.anchor(block) {
            Some(anchor) => self.layout().block_mask(self.puzzle.blocks[block].class, anchor),
            None => 0,
        }
    }

    /// The block covering `cell`, if any.
    pub fn block_at(&self, cell: Cell) -> Option<BlockId> {
        (0..self.anchors.len()).find(|&b| self.block_cells(b) & bit(cell) != 0)
    }

    /// Whether the block can take one step in each direction, indexed by
    /// [`Dir::index`].
    pub fn legal_steps(&self, block: BlockId) -> [bool; 4] {
        let mut out = [false; 4];
        if let Some(anchor) = self.anchor(block) {
            for dir in Dir::ALL {
                out[dir.index()] = self.layout().step_target(&self.position, anchor, dir).is_some();
            }
        }
        out
    }

    /// Anchors the block could be dragged to in one move.
    pub fn reachable(&self, block: BlockId) -> Mask {
        self.anchor(block).map_or(0, |a| self.layout().reachable(&self.position, a))
    }

    /// Moves a block one cell.
    pub fn step(&mut self, block: BlockId, dir: Dir) -> Result<(), MoveError> {
        let from = self.anchor(block).ok_or(MoveError::NoSuchBlock)?;
        let to = self
            .layout()
            .step_target(&self.position, from, dir)
            .ok_or(MoveError::Blocked)?;
        self.relocate(block, to);
        self.record(block, from, to);
        Ok(())
    }

    /// Moves a block straight to another anchor it can reach.
    pub fn drag_to(&mut self, block: BlockId, to: Cell) -> Result<(), MoveError> {
        let from = self.anchor(block).ok_or(MoveError::NoSuchBlock)?;
        if from == to {
            return Ok(());
        }
        if self.layout().reachable(&self.position, from) & bit(to) == 0 {
            return Err(MoveError::Blocked);
        }
        self.relocate(block, to);
        self.record(block, from, to);
        Ok(())
    }

    /// Plays a solver move, returning the block that made it.
    pub fn apply(&mut self, mv: Move) -> Result<BlockId, MoveError> {
        let block = self
            .anchors
            .iter()
            .position(|&a| a == mv.from)
            .filter(|&b| self.puzzle.blocks[b].class == mv.class)
            .ok_or(MoveError::NoSuchBlock)?;
        self.drag_to(block, mv.to)?;
        Ok(block)
    }

    fn relocate(&mut self, block: BlockId, to: Cell) {
        self.position.remove(self.anchors[block]);
        self.position.place(to, self.puzzle.blocks[block].class);
        self.anchors[block] = to;
    }

    fn record(&mut self, block: BlockId, from: Cell, to: Cell) {
        self.undone.clear();
        match self.history.last_mut() {
            Some(last) if last.block == block => {
                last.to = to;
                if last.to == last.from {
                    self.history.pop();
                }
            }
            _ => self.history.push(Drag { block, from, to }),
        }
    }

    /// Moves made so far, by the drag rule.
    pub fn move_count(&self) -> usize {
        self.history.len()
    }

    pub fn history(&self) -> &[Drag] {
        &self.history
    }

    /// Takes back the last counted move.
    pub fn undo(&mut self) -> Option<Drag> {
        let drag = self.history.pop()?;
        self.relocate(drag.block, drag.from);
        self.undone.push(drag);
        Some(drag)
    }

    /// Replays the move most recently undone.
    pub fn redo(&mut self) -> Option<Drag> {
        let drag = self.undone.pop()?;
        self.relocate(drag.block, drag.to);
        self.history.push(drag);
        Some(drag)
    }

    /// Back to the starting position with an empty history.
    pub fn reset(&mut self) {
        *self = Game::new(self.puzzle.clone());
    }

    pub fn is_solved(&self) -> bool {
        self.layout().is_goal(&self.position)
    }

    /// Jumps to an arbitrary position of this puzzle, clearing the history.
    /// Blocks of a class are matched to anchors in reading order.
    pub fn load_position(&mut self, position: &Position) -> Result<(), crate::position::PositionError> {
        self.layout().check_position(position)?;
        let mut free: Vec<(Cell, u8)> = position.blocks().collect();
        for (block, info) in self.puzzle.blocks.iter().enumerate() {
            let slot = free
                .iter()
                .position(|&(_, class)| class == info.class)
                .expect("counts checked");
            self.anchors[block] = free.remove(slot).0;
        }
        self.position = *position;
        self.history.clear();
        self.undone.clear();
        Ok(())
    }
}
