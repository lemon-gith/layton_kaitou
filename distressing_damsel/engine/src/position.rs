//! Positions and move generation.

use std::fmt;

use crate::geometry::{bit, cells, Cell, Dir, Mask, MAX_CELLS, NO_CELL};
use crate::layout::{ClassId, Layout, Symmetry};

/// Where every block is. Stores, for each cell, the class anchored there plus
/// one, or zero. Blocks of a class carry no identity, so two positions that
/// differ only by swapping identical blocks are the same value.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Position {
    cells: [u8; MAX_CELLS],
}

impl Position {
    pub const EMPTY: Position = Position { cells: [0; MAX_CELLS] };

    /// Class of the block anchored exactly at `anchor`, if any.
    #[inline]
    pub fn class_at(&self, anchor: Cell) -> Option<ClassId> {
        self.cells[anchor as usize].checked_sub(1)
    }

    #[inline]
    pub fn place(&mut self, anchor: Cell, class: ClassId) {
        self.cells[anchor as usize] = class + 1;
    }

    #[inline]
    pub fn remove(&mut self, anchor: Cell) {
        self.cells[anchor as usize] = 0;
    }

    /// `(anchor, class)` of every block, in anchor order.
    pub fn blocks(&self) -> impl Iterator<Item = (Cell, ClassId)> + '_ {
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, &c)| c != 0)
            .map(|(i, &c)| (i as Cell, c - 1))
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.blocks()).finish()
    }
}

/// One block changing anchor. Under [`Metric::Drag`] the two anchors may be
/// any distance apart.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Move {
    pub class: ClassId,
    pub from: Cell,
    pub to: Cell,
}

/// What counts as one move.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Metric {
    /// One block taken along any path while everything else stays put.
    #[default]
    Drag,
    /// One block moved one cell.
    Step,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositionError {
    /// A block is anchored where its class does not fit.
    BadPlacement(Cell),
    Overlap(Cell),
    /// The number of blocks of a class differs from the layout.
    WrongCount(ClassId),
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PositionError::BadPlacement(c) => write!(f, "block at cell {c} does not fit there"),
            PositionError::Overlap(c) => write!(f, "block at cell {c} overlaps another"),
            PositionError::WrongCount(c) => write!(f, "wrong number of blocks of class {c}"),
        }
    }
}

impl std::error::Error for PositionError {}

impl Layout {
    /// Cells covered by any block.
    pub fn occupancy(&self, position: &Position) -> Mask {
        position
            .blocks()
            .fold(0, |acc, (anchor, class)| acc | self.block_mask(class, anchor))
    }

    /// Anchor and class of the block covering `cell`.
    pub fn block_covering(&self, position: &Position, cell: Cell) -> Option<(Cell, ClassId)> {
        position
            .blocks()
            .find(|&(anchor, class)| self.block_mask(class, anchor) & bit(cell) != 0)
    }

    /// Where the block anchored at `anchor` lands after one step, if it can
    /// take it.
    pub fn step_target(&self, position: &Position, anchor: Cell, dir: Dir) -> Option<Cell> {
        let class = position.class_at(anchor)?;
        let table = self.class(class);
        let next = table.steps[dir.index()][anchor as usize];
        if next == NO_CELL {
            return None;
        }
        let others = self.occupancy(position) & !table.placement(anchor);
        (table.placement(next) & others == 0).then_some(next)
    }

    /// Anchors the block at `anchor` can be dragged to, excluding where it is.
    pub fn reachable(&self, position: &Position, anchor: Cell) -> Mask {
        match position.class_at(anchor) {
            Some(class) => {
                let others = self.occupancy(position) & !self.block_mask(class, anchor);
                self.flood(class, anchor, others)
            }
            None => 0,
        }
    }

    fn flood(&self, class: ClassId, anchor: Cell, others: Mask) -> Mask {
        let table = self.class(class);
        let mut seen = bit(anchor);
        let mut todo = seen;
        while todo != 0 {
            let cur = todo.trailing_zeros() as usize;
            todo &= todo - 1;
            for steps in &table.steps {
                let next = steps[cur];
                if next != NO_CELL
                    && seen & bit(next) == 0
                    && table.placements[next as usize] & others == 0
                {
                    seen |= bit(next);
                    todo |= bit(next);
                }
            }
        }
        seen & !bit(anchor)
    }

    /// A shortest sequence of steps taking the block at `from` to `to` with
    /// everything else fixed. `None` if it cannot get there.
    pub fn drag_path(&self, position: &Position, from: Cell, to: Cell) -> Option<Vec<Dir>> {
        let class = position.class_at(from)?;
        let table = self.class(class);
        let others = self.occupancy(position) & !table.placement(from);
        let mut came_by = [None::<Dir>; MAX_CELLS];
        let mut seen = bit(from);
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some(cur) = queue.pop_front() {
            if cur == to {
                let mut path = Vec::new();
                let mut at = to;
                while let Some(dir) = came_by[at as usize] {
                    path.push(dir);
                    at = table.steps[dir.opposite().index()][at as usize];
                }
                path.reverse();
                return Some(path);
            }
            for dir in Dir::ALL {
                let next = table.steps[dir.index()][cur as usize];
                if next != NO_CELL
                    && seen & bit(next) == 0
                    && table.placements[next as usize] & others == 0
                {
                    seen |= bit(next);
                    came_by[next as usize] = Some(dir);
                    queue.push_back(next);
                }
            }
        }
        None
    }

    /// Calls `visit` with every move available and the position it leads to.
    pub fn for_each_successor(
        &self,
        position: &Position,
        metric: Metric,
        mut visit: impl FnMut(Move, &Position),
    ) {
        let occupied = self.occupancy(position);
        for (anchor, class) in position.blocks() {
            let table = self.class(class);
            let others = occupied & !table.placement(anchor);
            let targets = match metric {
                Metric::Drag => self.flood(class, anchor, others),
                Metric::Step => table.steps.iter().fold(0, |acc, steps| {
                    let next = steps[anchor as usize];
                    if next != NO_CELL && table.placements[next as usize] & others == 0 {
                        acc | bit(next)
                    } else {
                        acc
                    }
                }),
            };
            for to in cells(targets) {
                let mut next = *position;
                next.remove(anchor);
                next.place(to, class);
                visit(Move { class, from: anchor, to }, &next);
            }
        }
    }

    /// The position after `mv`, or `None` if the move is not legal here.
    pub fn apply(&self, position: &Position, mv: Move, metric: Metric) -> Option<Position> {
        if position.class_at(mv.from) != Some(mv.class) {
            return None;
        }
        let legal = match metric {
            Metric::Drag => self.reachable(position, mv.from) & bit(mv.to) != 0,
            Metric::Step => Dir::ALL
                .iter()
                .any(|&d| self.step_target(position, mv.from, d) == Some(mv.to)),
        };
        legal.then(|| {
            let mut next = *position;
            next.remove(mv.from);
            next.place(mv.to, mv.class);
            next
        })
    }

    pub fn is_goal(&self, position: &Position) -> bool {
        self.goal().is_met(position)
    }

    /// Checks that `position` is a legal arrangement of this layout's blocks.
    pub fn check_position(&self, position: &Position) -> Result<(), PositionError> {
        let mut counts = [0u16; 256];
        let mut covered = 0;
        for (anchor, class) in position.blocks() {
            let mask = match self.classes().get(class as usize) {
                Some(c) if (anchor as usize) < self.n_cells() => c.placement(anchor),
                _ => 0,
            };
            if mask == 0 {
                return Err(PositionError::BadPlacement(anchor));
            }
            if mask & covered != 0 {
                return Err(PositionError::Overlap(anchor));
            }
            covered |= mask;
            counts[class as usize] += 1;
        }
        for (id, class) in self.classes().iter().enumerate() {
            if counts[id] != class.count as u16 {
                return Err(PositionError::WrongCount(id as ClassId));
            }
        }
        Ok(())
    }

    /// Image of a position under one of this layout's symmetries.
    pub fn transform(&self, position: &Position, symmetry: &Symmetry) -> Position {
        let mut out = Position::EMPTY;
        for (anchor, class) in position.blocks() {
            let (class, anchor) = symmetry.map(class, anchor);
            out.place(anchor, class);
        }
        out
    }

    /// A representative of `mv` that is the same for every mirror image of
    /// it, so that moves can be compared across symmetric positions.
    pub fn canonical_move(&self, mv: Move) -> Move {
        self.symmetries().iter().fold(mv, |best, s| {
            let (class, from) = s.map(mv.class, mv.from);
            let (_, to) = s.map(mv.class, mv.to);
            best.min(Move { class, from, to })
        })
    }
}
