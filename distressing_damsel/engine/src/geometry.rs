//! Cells, bit masks, directions and board transforms.

use serde::{Deserialize, Serialize};

/// Row-major cell index: `y * width + x`.
pub type Cell = u8;
/// One bit per cell.
pub type Mask = u64;

/// Largest board this representation supports.
pub const MAX_CELLS: usize = 64;
/// Sentinel for "no such cell" in lookup tables.
pub const NO_CELL: Cell = u8::MAX;

#[inline]
pub const fn bit(cell: Cell) -> Mask {
    1u64 << cell
}

/// Iterates the cells set in a mask, lowest first.
pub fn cells(mask: Mask) -> Cells {
    Cells(mask)
}

#[derive(Clone)]
pub struct Cells(Mask);

impl Iterator for Cells {
    type Item = Cell;

    #[inline]
    fn next(&mut self) -> Option<Cell> {
        if self.0 == 0 {
            return None;
        }
        let cell = self.0.trailing_zeros() as Cell;
        self.0 &= self.0 - 1;
        Some(cell)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    /// `(dx, dy)` with y growing downwards.
    pub const fn delta(self) -> (i8, i8) {
        match self {
            Dir::Up => (0, -1),
            Dir::Down => (0, 1),
            Dir::Left => (-1, 0),
            Dir::Right => (1, 0),
        }
    }

    pub const fn opposite(self) -> Dir {
        match self {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
        }
    }

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn arrow(self) -> char {
        match self {
            Dir::Up => '↑',
            Dir::Down => '↓',
            Dir::Left => '←',
            Dir::Right => '→',
        }
    }
}

/// Movement restriction on a class of blocks.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    #[default]
    Free,
    Horizontal,
    Vertical,
}

impl Axis {
    pub const fn allows(self, dir: Dir) -> bool {
        match self {
            Axis::Free => true,
            Axis::Horizontal => matches!(dir, Dir::Left | Dir::Right),
            Axis::Vertical => matches!(dir, Dir::Up | Dir::Down),
        }
    }

    pub const fn swapped(self) -> Axis {
        match self {
            Axis::Free => Axis::Free,
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }
}

/// The eight symmetries of a rectangle. The last four need a square board.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Transform {
    Identity,
    FlipH,
    FlipV,
    Rot180,
    Transpose,
    AntiTranspose,
    Rot90,
    Rot270,
}

impl Transform {
    pub const ALL: [Transform; 8] = [
        Transform::Identity,
        Transform::FlipH,
        Transform::FlipV,
        Transform::Rot180,
        Transform::Transpose,
        Transform::AntiTranspose,
        Transform::Rot90,
        Transform::Rot270,
    ];

    /// Whether rows and columns trade places.
    pub const fn swaps_axes(self) -> bool {
        matches!(
            self,
            Transform::Transpose | Transform::AntiTranspose | Transform::Rot90 | Transform::Rot270
        )
    }

    /// Image of `(x, y)` on a `w` by `h` board.
    pub const fn apply(self, x: u8, y: u8, w: u8, h: u8) -> (u8, u8) {
        match self {
            Transform::Identity => (x, y),
            Transform::FlipH => (w - 1 - x, y),
            Transform::FlipV => (x, h - 1 - y),
            Transform::Rot180 => (w - 1 - x, h - 1 - y),
            Transform::Transpose => (y, x),
            Transform::AntiTranspose => (h - 1 - y, w - 1 - x),
            Transform::Rot90 => (h - 1 - y, x),
            Transform::Rot270 => (y, w - 1 - x),
        }
    }
}
