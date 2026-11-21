//! Puzzle descriptions as a person writes them, and their validation.
//!
//! A [`PuzzleSpec`] is the editable form: a grid of labelled cells plus
//! per-block attributes and goal targets. [`PuzzleSpec::build`] checks it and
//! produces a [`Puzzle`] ready to play or solve.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::bits::{base64_decode, base64_encode, bits_for, BitReader, BitWriter};
use crate::geometry::{bit, cells, Axis, Cell, Dir, Mask, MAX_CELLS};
use crate::layout::{ClassDef, ClassId, Goal, Layout, LayoutError, Shape};
use crate::position::Position;

const CODE_PREFIX: &str = "dd1-";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CellSpec {
    Wall,
    Empty,
    /// Part of the block with this label.
    Block(char),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockAttrs {
    /// Blocks sharing a tag (and shape) are interchangeable for goals.
    pub tag: Option<String>,
    pub axis: Axis,
}

/// "The block labelled `block` (or one interchangeable with it) must end with
/// the top-left corner of its bounding box at `(x, y)`."
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalSpec {
    pub block: char,
    pub x: u8,
    pub y: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PuzzleSpec {
    pub name: String,
    pub description: String,
    pub width: u8,
    pub height: u8,
    /// Row-major, `width * height` entries.
    pub cells: Vec<CellSpec>,
    /// Attributes for blocks that need any. Blocks not listed are free-moving
    /// and untagged.
    pub blocks: BTreeMap<char, BlockAttrs>,
    /// All of these must hold at once.
    pub goals: Vec<GoalSpec>,
    /// Side of the board the key block leaves by once the puzzle is solved.
    /// Only for single-target puzzles; inferred when not given.
    pub exit: Option<Dir>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecError {
    EmptyGrid,
    /// Row (zero-based) whose length differs from the first row.
    RaggedRow(usize),
    TooLarge { cells: usize },
    BadCharacter { ch: char, row: usize, column: usize },
    /// Cells carrying this label are not one connected piece.
    Disconnected(char),
    /// Attributes or a goal name a label that is not in the grid.
    UnknownBlock(char),
    /// Tags starting with `@` are reserved.
    ReservedTag(String),
    NoGoal,
    DuplicateGoal(char),
    /// The block would stick out of the board or into a wall at its target.
    GoalOutOfBounds(char),
    /// Two targets claim the same cell.
    GoalOverlap(char, char),
    TooManyBlocks,
    /// An exit was given for a puzzle with several targets, or on a side the
    /// target does not touch.
    BadExit,
    Layout(LayoutError),
    /// A share code that cannot be read.
    BadCode,
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecError::EmptyGrid => write!(f, "the grid is empty"),
            SpecError::RaggedRow(r) => write!(f, "grid row {} is not the same length as the first row", r + 1),
            SpecError::TooLarge { cells } => {
                write!(f, "the grid has {cells} cells; the limit is {MAX_CELLS}")
            }
            SpecError::BadCharacter { ch, row, column } => write!(
                f,
                "unexpected character {ch:?} at row {}, column {} (use letters or digits for blocks, '.' for empty, '#' for walls)",
                row + 1,
                column + 1
            ),
            SpecError::Disconnected(l) => write!(f, "block {l} is not one connected piece"),
            SpecError::UnknownBlock(l) => write!(f, "block {l} is not in the grid"),
            SpecError::ReservedTag(t) => write!(f, "tag {t:?} is reserved (tags may not start with '@')"),
            SpecError::NoGoal => write!(f, "the puzzle has no goal"),
            SpecError::DuplicateGoal(l) => write!(f, "block {l} has more than one goal"),
            SpecError::GoalOutOfBounds(l) => write!(f, "the goal for block {l} is off the board or inside a wall"),
            SpecError::GoalOverlap(a, b) => write!(f, "the goals for blocks {a} and {b} overlap"),
            SpecError::TooManyBlocks => write!(f, "too many blocks of one kind (limit 255)"),
            SpecError::BadExit => write!(
                f,
                "the exit must be on a side of the board that the puzzle's only target touches"
            ),
            SpecError::Layout(e) => write!(f, "{e}"),
            SpecError::BadCode => write!(f, "not a valid puzzle code"),
        }
    }
}

impl std::error::Error for SpecError {}

/// Things worth telling the author that do not stop the puzzle being used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    /// Open cells that no block can ever enter because they are cut off from
    /// every block. They can be turned into walls without changing anything.
    DeadCells(Mask),
    /// The starting position already satisfies the goal.
    AlreadySolved,
}

/// One block of a built puzzle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockInfo {
    pub label: char,
    pub class: ClassId,
    /// Anchor in the starting position.
    pub start: Cell,
}

/// A validated puzzle.
#[derive(Clone, Debug)]
pub struct Puzzle {
    pub name: String,
    pub description: String,
    pub layout: Arc<Layout>,
    pub start: Position,
    /// Indexed by [`crate::BlockId`], in label order.
    pub blocks: Vec<BlockInfo>,
    /// Side the key block leaves by when solved, if it has a way out.
    pub exit: Option<Dir>,
}

impl PuzzleSpec {
    /// Reads a grid drawn in text: one row per line, letters or digits for
    /// blocks (same character, same block), `.` for empty, `#` for walls.
    pub fn from_grid(grid: &str) -> Result<PuzzleSpec, SpecError> {
        let rows: Vec<&str> = grid.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let first = rows.first().ok_or(SpecError::EmptyGrid)?;
        let width = first.chars().count();
        let mut cells = Vec::with_capacity(width * rows.len());
        for (row, line) in rows.iter().enumerate() {
            if line.chars().count() != width {
                return Err(SpecError::RaggedRow(row));
            }
            for (column, ch) in line.chars().enumerate() {
                cells.push(match ch {
                    '#' => CellSpec::Wall,
                    '.' => CellSpec::Empty,
                    c if c.is_ascii_alphanumeric() => CellSpec::Block(c),
                    ch => return Err(SpecError::BadCharacter { ch, row, column }),
                });
            }
        }
        if cells.len() > MAX_CELLS {
            return Err(SpecError::TooLarge { cells: cells.len() });
        }
        Ok(PuzzleSpec {
            width: width as u8,
            height: rows.len() as u8,
            cells,
            ..PuzzleSpec::default()
        })
    }

    /// The grid as text, in the form [`PuzzleSpec::from_grid`] reads.
    pub fn grid_string(&self) -> String {
        let mut out = String::new();
        for row in self.cells.chunks(self.width.max(1) as usize) {
            out.extend(row.iter().map(|c| match c {
                CellSpec::Wall => '#',
                CellSpec::Empty => '.',
                CellSpec::Block(l) => *l,
            }));
            out.push('\n');
        }
        out
    }

    fn check_grid(&self) -> Result<(), SpecError> {
        let n = self.width as usize * self.height as usize;
        if n == 0 || self.cells.len() != n {
            return Err(SpecError::EmptyGrid);
        }
        if n > MAX_CELLS {
            return Err(SpecError::TooLarge { cells: n });
        }
        Ok(())
    }

    /// Cells of each block, by label.
    fn block_cells(&self) -> BTreeMap<char, Vec<(u8, u8)>> {
        let mut blocks: BTreeMap<char, Vec<(u8, u8)>> = BTreeMap::new();
        for (i, cell) in self.cells.iter().enumerate() {
            if let CellSpec::Block(label) = cell {
                let (x, y) = (i % self.width as usize, i / self.width as usize);
                blocks.entry(*label).or_default().push((x as u8, y as u8));
            }
        }
        blocks
    }

    /// Validates the description and builds the puzzle.
    pub fn build(&self) -> Result<(Puzzle, Vec<Warning>), SpecError> {
        self.check_grid()?;
        let (w, h) = (self.width, self.height);
        let cell_of = |x: u8, y: u8| y * w + x;

        let mut open: Mask = 0;
        for (i, cell) in self.cells.iter().enumerate() {
            if *cell != CellSpec::Wall {
                open |= bit(i as Cell);
            }
        }

        let block_cells = self.block_cells();
        for label in self.blocks.keys().chain(self.goals.iter().map(|g| &g.block)) {
            if !block_cells.contains_key(label) {
                return Err(SpecError::UnknownBlock(*label));
            }
        }
        if self.goals.is_empty() {
            return Err(SpecError::NoGoal);
        }

        // Group blocks into classes. A block named by a goal is kept apart
        // from look-alikes unless the author tagged it into a group.
        let mut defs: Vec<ClassDef> = Vec::new();
        let mut blocks = Vec::with_capacity(block_cells.len());
        for (&label, cells) in &block_cells {
            let (shape, (ax, ay)) = Shape::from_cells(cells.iter().copied()).expect("label has cells");
            if !shape.is_connected() {
                return Err(SpecError::Disconnected(label));
            }
            let attrs = self.blocks.get(&label).cloned().unwrap_or_default();
            if let Some(tag) = attrs.tag.as_ref().filter(|t| t.starts_with('@')) {
                return Err(SpecError::ReservedTag(tag.clone()));
            }
            let is_goal = self.goals.iter().any(|g| g.block == label);
            let tag = attrs.tag.or_else(|| is_goal.then(|| format!("@{label}")));
            let class = match defs
                .iter()
                .position(|d| d.shape == shape && d.tag == tag && d.axis == attrs.axis)
            {
                Some(i) => {
                    defs[i].count = defs[i].count.checked_add(1).ok_or(SpecError::TooManyBlocks)?;
                    i
                }
                None => {
                    defs.push(ClassDef { shape, tag, axis: attrs.axis, count: 1 });
                    defs.len() - 1
                }
            };
            blocks.push(BlockInfo { label, class: class as ClassId, start: cell_of(ax, ay) });
        }

        // Resolve goal targets to anchors.
        let mut targets: Vec<(char, ClassId, Cell, Mask)> = Vec::new();
        for goal in &self.goals {
            if targets.iter().any(|t| t.0 == goal.block) {
                return Err(SpecError::DuplicateGoal(goal.block));
            }
            let info = blocks.iter().find(|b| b.label == goal.block).expect("checked above");
            let shape = &defs[info.class as usize].shape;
            let (min_dx, _, _) = shape.bounds();
            let ax = goal.x as i16 - min_dx as i16;
            let mut mask: Mask = 0;
            for &(dx, dy) in shape.offsets() {
                let (x, y) = (ax + dx as i16, goal.y as i16 + dy as i16);
                if x < 0 || x >= w as i16 || y < 0 || y >= h as i16 {
                    return Err(SpecError::GoalOutOfBounds(goal.block));
                }
                mask |= bit(cell_of(x as u8, y as u8));
            }
            if mask & !open != 0 {
                return Err(SpecError::GoalOutOfBounds(goal.block));
            }
            if let Some(other) = targets.iter().find(|t| t.3 & mask != 0) {
                return Err(SpecError::GoalOverlap(other.0, goal.block));
            }
            targets.push((goal.block, info.class, cell_of(ax as u8, goal.y), mask));
        }
        let exit = match targets.as_slice() {
            [(_, _, _, mask)] => {
                let touches = |dir: Dir| -> bool {
                    cells(*mask).any(|c| {
                        let (x, y) = ((c % w) as i16, (c / w) as i16);
                        let (dx, dy) = dir.delta();
                        let (nx, ny) = (x + dx as i16, y + dy as i16);
                        nx < 0 || ny < 0 || nx >= w as i16 || ny >= h as i16
                    })
                };
                match self.exit {
                    Some(dir) if touches(dir) => Some(dir),
                    Some(_) => return Err(SpecError::BadExit),
                    None => [Dir::Down, Dir::Right, Dir::Up, Dir::Left].into_iter().find(|&d| touches(d)),
                }
            }
            _ if self.exit.is_some() => return Err(SpecError::BadExit),
            _ => None,
        };
        let mut parts: Vec<Goal> = targets
            .iter()
            .map(|&(_, class, anchor, _)| Goal::BlockAt { class, anchor })
            .collect();
        let goal = if parts.len() == 1 { parts.remove(0) } else { Goal::All(parts) };

        let layout = Layout::new(w, h, open, defs, goal).map_err(SpecError::Layout)?;
        let mut start = Position::EMPTY;
        for block in &blocks {
            start.place(block.start, block.class);
        }

        let mut warnings = Vec::new();
        let dead = dead_cells(&layout, &start);
        if dead != 0 {
            warnings.push(Warning::DeadCells(dead));
        }
        if layout.is_goal(&start) {
            warnings.push(Warning::AlreadySolved);
        }

        let puzzle = Puzzle {
            name: self.name.clone(),
            description: self.description.clone(),
            layout: Arc::new(layout),
            start,
            blocks,
            exit,
        };
        Ok((puzzle, warnings))
    }

    /// A copy with the given cells turned into walls. Use with
    /// [`Warning::DeadCells`] to tidy a board.
    pub fn with_walls(&self, mask: Mask) -> PuzzleSpec {
        let mut spec = self.clone();
        for cell in cells(mask) {
            if let Some(c) = spec.cells.get_mut(cell as usize) {
                *c = CellSpec::Wall;
            }
        }
        spec
    }

    /// A compact copy-and-paste code for the puzzle. Carries the grid, block
    /// labels, axes, tag groups and goals; not the name, description or tag
    /// names.
    pub fn to_code(&self) -> Result<String, SpecError> {
        self.check_grid()?;
        let labels: Vec<char> = self.block_cells().into_keys().collect();
        if labels.len() > 254 || labels.iter().any(|l| !l.is_ascii()) {
            return Err(SpecError::TooManyBlocks);
        }
        let index = |label: char| labels.iter().position(|&l| l == label);
        let mut tags: Vec<&str> = Vec::new();

        let mut bytes = Vec::new();
        let mut w = BitWriter::new(&mut bytes);
        w.write(self.width as u32, 8);
        w.write(self.height as u32, 8);
        w.write(labels.len() as u32, 8);
        let cell_bits = bits_for(labels.len() + 1);
        for cell in &self.cells {
            let symbol = match cell {
                CellSpec::Wall => 0,
                CellSpec::Empty => 1,
                CellSpec::Block(l) => index(*l).expect("label collected") + 2,
            };
            w.write(symbol as u32, cell_bits);
        }
        for &label in &labels {
            let attrs = self.blocks.get(&label).cloned().unwrap_or_default();
            let tag = match attrs.tag.as_deref() {
                None => 0,
                Some(t) => match tags.iter().position(|&k| k == t) {
                    Some(i) => i + 1,
                    None => {
                        tags.push(self.blocks[&label].tag.as_deref().expect("tag present"));
                        tags.len()
                    }
                },
            };
            w.write(label as u32, 8);
            w.write(attrs.axis as u32, 2);
            w.write(tag.min(255) as u32, 8);
        }
        w.write(self.goals.len().min(255) as u32, 8);
        for goal in self.goals.iter().take(255) {
            let block = index(goal.block).ok_or(SpecError::UnknownBlock(goal.block))?;
            w.write(block as u32, 8);
            w.write(goal.x as u32, 8);
            w.write(goal.y as u32, 8);
        }
        w.write(self.exit.map_or(0, |d| d.index() as u32 + 1), 3);
        w.finish();
        Ok(format!("{CODE_PREFIX}{}", base64_encode(&bytes)))
    }

    /// Reads a code made by [`PuzzleSpec::to_code`]. Tag groups come back
    /// named `t1`, `t2`, and so on.
    pub fn from_code(code: &str) -> Result<PuzzleSpec, SpecError> {
        let body = code.trim().strip_prefix(CODE_PREFIX).ok_or(SpecError::BadCode)?;
        let bytes = base64_decode(body).ok_or(SpecError::BadCode)?;
        let mut r = BitReader::new(&bytes);
        let mut read = |bits| r.read(bits).ok_or(SpecError::BadCode);

        let width = read(8)? as u8;
        let height = read(8)? as u8;
        let n_blocks = read(8)? as usize;
        let n_cells = width as usize * height as usize;
        if n_cells == 0 || n_cells > MAX_CELLS {
            return Err(SpecError::BadCode);
        }
        let cell_bits = bits_for(n_blocks + 1);
        let mut raw = Vec::with_capacity(n_cells);
        for _ in 0..n_cells {
            raw.push(read(cell_bits)? as usize);
        }
        let mut labels = Vec::with_capacity(n_blocks);
        let mut blocks = BTreeMap::new();
        for _ in 0..n_blocks {
            let label = char::from(read(8)? as u8);
            let axis = match read(2)? {
                0 => Axis::Free,
                1 => Axis::Horizontal,
                2 => Axis::Vertical,
                _ => return Err(SpecError::BadCode),
            };
            let tag = match read(8)? {
                0 => None,
                t => Some(format!("t{t}")),
            };
            if !label.is_ascii_alphanumeric() || labels.contains(&label) {
                return Err(SpecError::BadCode);
            }
            if tag.is_some() || axis != Axis::Free {
                blocks.insert(label, BlockAttrs { tag, axis });
            }
            labels.push(label);
        }
        let cells = raw
            .into_iter()
            .map(|s| match s {
                0 => Ok(CellSpec::Wall),
                1 => Ok(CellSpec::Empty),
                s => labels.get(s - 2).map(|&l| CellSpec::Block(l)).ok_or(SpecError::BadCode),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut goals = Vec::new();
        for _ in 0..read(8)? {
            let block = *labels.get(read(8)? as usize).ok_or(SpecError::BadCode)?;
            goals.push(GoalSpec { block, x: read(8)? as u8, y: read(8)? as u8 });
        }
        let exit = match r.read(3) {
            Some(n @ 1..=4) => Some(Dir::ALL[n as usize - 1]),
            _ => None,
        };
        Ok(PuzzleSpec { width, height, cells, blocks, goals, exit, ..PuzzleSpec::default() })
    }
}

/// Open cells in regions that contain no block at all.
fn dead_cells(layout: &Layout, start: &Position) -> Mask {
    let occupied = layout.occupancy(start);
    let mut unvisited = layout.open();
    let mut dead = 0;
    while unvisited != 0 {
        let seed = unvisited.trailing_zeros() as Cell;
        let mut region = bit(seed);
        let mut stack = vec![seed];
        while let Some(cell) = stack.pop() {
            let (x, y) = layout.xy(cell);
            for dir in Dir::ALL {
                let (dx, dy) = dir.delta();
                let (nx, ny) = (x as i16 + dx as i16, y as i16 + dy as i16);
                if nx < 0 || ny < 0 || nx >= layout.width() as i16 || ny >= layout.height() as i16 {
                    continue;
                }
                let next = layout.cell(nx as u8, ny as u8);
                if layout.open() & bit(next) != 0 && region & bit(next) == 0 {
                    region |= bit(next);
                    stack.push(next);
                }
            }
        }
        if region & occupied == 0 {
            dead |= region;
        }
        unvisited &= !region;
    }
    dead
}
