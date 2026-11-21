//! The immutable part of a puzzle: board, block classes, goal, symmetries.

use std::fmt;

use crate::bits::bits_for;
use crate::geometry::{bit, cells, Axis, Cell, Dir, Mask, Transform, MAX_CELLS, NO_CELL};
use crate::position::Position;

/// Index into [`Layout::classes`].
pub type ClassId = u8;

/// Class ids are stored plus one in a byte, with zero meaning "no block".
pub const MAX_CLASSES: usize = 254;

/// A set of cells relative to an anchor. The anchor is the shape's first cell
/// in row-major order, so every offset has `dy >= 0`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shape {
    offsets: Vec<(i8, i8)>,
}

impl Shape {
    /// Builds a shape from absolute cells, returning it with its anchor.
    /// `None` if there are no cells.
    pub fn from_cells(cells: impl IntoIterator<Item = (u8, u8)>) -> Option<(Shape, (u8, u8))> {
        let mut cells: Vec<(u8, u8)> = cells.into_iter().collect();
        cells.sort_by_key(|&(x, y)| (y, x));
        cells.dedup();
        let &(ax, ay) = cells.first()?;
        let offsets = cells
            .iter()
            .map(|&(x, y)| (x as i8 - ax as i8, y as i8 - ay as i8))
            .collect();
        Some((Shape { offsets }, (ax, ay)))
    }

    pub fn rect(width: u8, height: u8) -> Shape {
        let cells = (0..height).flat_map(|y| (0..width).map(move |x| (x, y)));
        Shape::from_cells(cells).expect("rectangle has cells").0
    }

    /// `(dx, dy)` of each cell from the anchor, in row-major order.
    pub fn offsets(&self) -> &[(i8, i8)] {
        &self.offsets
    }

    pub fn area(&self) -> usize {
        self.offsets.len()
    }

    /// `(min_dx, width, height)` of the bounding box. `min_dx` is zero or
    /// negative: an irregular shape can extend left of its anchor.
    pub fn bounds(&self) -> (i8, u8, u8) {
        let min_dx = self.offsets.iter().map(|o| o.0).min().unwrap_or(0);
        let max_dx = self.offsets.iter().map(|o| o.0).max().unwrap_or(0);
        let max_dy = self.offsets.iter().map(|o| o.1).max().unwrap_or(0);
        (min_dx, (max_dx - min_dx + 1) as u8, (max_dy + 1) as u8)
    }

    /// Whether the cells form one edge-connected piece.
    pub fn is_connected(&self) -> bool {
        let mut seen = vec![false; self.offsets.len()];
        let mut stack = vec![0usize];
        seen[0] = true;
        let mut count = 1;
        while let Some(i) = stack.pop() {
            let (x, y) = self.offsets[i];
            for dir in Dir::ALL {
                let (dx, dy) = dir.delta();
                let next = (x + dx, y + dy);
                if let Some(j) = self.offsets.iter().position(|&o| o == next) {
                    if !seen[j] {
                        seen[j] = true;
                        count += 1;
                        stack.push(j);
                    }
                }
            }
        }
        count == self.offsets.len()
    }
}

/// What a caller supplies to describe a class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDef {
    pub shape: Shape,
    /// Blocks with the same shape but different tags are distinguishable.
    pub tag: Option<String>,
    pub axis: Axis,
    /// How many blocks of this class are on the board.
    pub count: u8,
}

/// A class of interchangeable blocks, with its lookup tables.
#[derive(Clone)]
pub struct Class {
    pub shape: Shape,
    pub tag: Option<String>,
    pub axis: Axis,
    pub count: u8,
    /// Cells covered when anchored at each cell; zero where it does not fit.
    pub(crate) placements: [Mask; MAX_CELLS],
    /// Anchor after one step in each direction, or `NO_CELL`.
    pub(crate) steps: [[Cell; MAX_CELLS]; 4],
}

impl Class {
    /// Cells covered by a block of this class anchored at `anchor`, or zero
    /// if it does not fit on the board there.
    #[inline]
    pub fn placement(&self, anchor: Cell) -> Mask {
        self.placements[anchor as usize]
    }
}

impl fmt::Debug for Class {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Class")
            .field("shape", &self.shape)
            .field("tag", &self.tag)
            .field("axis", &self.axis)
            .field("count", &self.count)
            .finish()
    }
}

/// Condition that ends the puzzle. A predicate over positions, so the solver
/// never needs to know which kind it is.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Goal {
    /// Some block of `class` is anchored at `anchor`.
    BlockAt { class: ClassId, anchor: Cell },
    All(Vec<Goal>),
    Any(Vec<Goal>),
}

impl Goal {
    pub fn is_met(&self, position: &Position) -> bool {
        match self {
            Goal::BlockAt { class, anchor } => position.class_at(*anchor) == Some(*class),
            Goal::All(parts) => parts.iter().all(|g| g.is_met(position)),
            Goal::Any(parts) => parts.iter().any(|g| g.is_met(position)),
        }
    }

    /// Every `(class, anchor)` the goal mentions.
    pub fn targets(&self) -> Vec<(ClassId, Cell)> {
        fn walk(goal: &Goal, out: &mut Vec<(ClassId, Cell)>) {
            match goal {
                Goal::BlockAt { class, anchor } => out.push((*class, *anchor)),
                Goal::All(parts) | Goal::Any(parts) => parts.iter().for_each(|g| walk(g, out)),
            }
        }
        let mut out = Vec::new();
        walk(self, &mut out);
        out
    }

    fn map(&self, f: &impl Fn(ClassId, Cell) -> (ClassId, Cell)) -> Goal {
        match self {
            Goal::BlockAt { class, anchor } => {
                let (class, anchor) = f(*class, *anchor);
                Goal::BlockAt { class, anchor }
            }
            Goal::All(parts) => Goal::All(parts.iter().map(|g| g.map(f)).collect()),
            Goal::Any(parts) => Goal::Any(parts.iter().map(|g| g.map(f)).collect()),
        }
    }

    /// Order-independent form, for comparing goals.
    fn normalized(&self) -> Goal {
        let sorted = |parts: &[Goal]| {
            let mut parts: Vec<Goal> = parts.iter().map(Goal::normalized).collect();
            parts.sort();
            parts.dedup();
            parts
        };
        match self {
            Goal::BlockAt { .. } => self.clone(),
            Goal::All(parts) => Goal::All(sorted(parts)),
            Goal::Any(parts) => Goal::Any(sorted(parts)),
        }
    }
}

/// A board transform that maps the puzzle onto itself, so that a position and
/// its image are equally far from the goal.
#[derive(Clone)]
pub struct Symmetry {
    pub transform: Transform,
    /// Class each class becomes.
    pub(crate) class_perm: Vec<ClassId>,
    /// Per class, where an anchor ends up.
    pub(crate) anchor_map: Vec<[Cell; MAX_CELLS]>,
}

impl Symmetry {
    #[inline]
    pub(crate) fn map(&self, class: ClassId, anchor: Cell) -> (ClassId, Cell) {
        (
            self.class_perm[class as usize],
            self.anchor_map[class as usize][anchor as usize],
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// Board is empty, larger than 64 cells, or has open cells outside it.
    BadDimensions,
    TooManyClasses,
    /// Two classes share shape, tag and axis.
    DuplicateClass(usize),
    EmptyClass(usize),
    DisconnectedShape(usize),
    /// A class has no position on the board where it fits.
    ShapeDoesNotFit(usize),
    /// Blocks cover more cells than the board has.
    Overfull,
    /// The goal names a class or placement that does not exist.
    BadGoal,
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::BadDimensions => write!(f, "board must have between 1 and {MAX_CELLS} cells"),
            LayoutError::TooManyClasses => write!(f, "more than {MAX_CLASSES} distinct block classes"),
            LayoutError::DuplicateClass(i) => write!(f, "class {i} duplicates an earlier class"),
            LayoutError::EmptyClass(i) => write!(f, "class {i} has no blocks or no cells"),
            LayoutError::DisconnectedShape(i) => write!(f, "class {i} is not one connected piece"),
            LayoutError::ShapeDoesNotFit(i) => write!(f, "class {i} does not fit anywhere on the board"),
            LayoutError::Overfull => write!(f, "blocks cover more cells than the board has"),
            LayoutError::BadGoal => write!(f, "goal refers to a class or placement that does not exist"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// Board, classes and goal, with everything move generation and encoding need
/// precomputed.
#[derive(Clone)]
pub struct Layout {
    width: u8,
    height: u8,
    open: Mask,
    classes: Vec<Class>,
    goal: Goal,
    symmetries: Vec<Symmetry>,
    n_blocks: usize,
    pub(crate) n_symbols: usize,
    pub(crate) symbol_bits: u32,
}

impl fmt::Debug for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Layout")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("open", &format_args!("{:#x}", self.open))
            .field("classes", &self.classes)
            .field("goal", &self.goal)
            .finish()
    }
}

impl Layout {
    /// `open` has a bit set for every cell that is not a wall.
    pub fn new(
        width: u8,
        height: u8,
        open: Mask,
        defs: Vec<ClassDef>,
        goal: Goal,
    ) -> Result<Layout, LayoutError> {
        let n_cells = width as usize * height as usize;
        if n_cells == 0 || n_cells > MAX_CELLS {
            return Err(LayoutError::BadDimensions);
        }
        let board = if n_cells == MAX_CELLS { !0 } else { (1u64 << n_cells) - 1 };
        if open & !board != 0 {
            return Err(LayoutError::BadDimensions);
        }
        if defs.len() > MAX_CLASSES {
            return Err(LayoutError::TooManyClasses);
        }

        let mut classes = Vec::with_capacity(defs.len());
        let mut area = 0usize;
        let mut n_blocks = 0usize;
        for (i, def) in defs.iter().enumerate() {
            if def.count == 0 || def.shape.area() == 0 {
                return Err(LayoutError::EmptyClass(i));
            }
            if !def.shape.is_connected() {
                return Err(LayoutError::DisconnectedShape(i));
            }
            let same = |d: &ClassDef| d.shape == def.shape && d.tag == def.tag && d.axis == def.axis;
            if defs[..i].iter().any(same) {
                return Err(LayoutError::DuplicateClass(i));
            }
            let class = build_class(def, width, height, open);
            if class.placements.iter().all(|&m| m == 0) {
                return Err(LayoutError::ShapeDoesNotFit(i));
            }
            area += def.shape.area() * def.count as usize;
            n_blocks += def.count as usize;
            classes.push(class);
        }
        let open_cells = open.count_ones() as usize;
        if area > open_cells {
            return Err(LayoutError::Overfull);
        }
        for (class, anchor) in goal.targets() {
            let fits = classes
                .get(class as usize)
                .is_some_and(|c| (anchor as usize) < n_cells && c.placement(anchor) != 0);
            if !fits {
                return Err(LayoutError::BadGoal);
            }
        }

        let mut layout = Layout {
            width,
            height,
            open,
            classes,
            goal,
            symmetries: Vec::new(),
            n_blocks,
            n_symbols: n_blocks + (open_cells - area),
            symbol_bits: bits_for(defs.len()),
        };
        layout.symmetries = Transform::ALL[1..]
            .iter()
            .filter_map(|&t| layout.symmetry(t))
            .collect();
        Ok(layout)
    }

    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    pub fn n_cells(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Cells that are not walls.
    pub fn open(&self) -> Mask {
        self.open
    }

    pub fn is_wall(&self, cell: Cell) -> bool {
        self.open & bit(cell) == 0
    }

    pub fn classes(&self) -> &[Class] {
        &self.classes
    }

    pub fn class(&self, id: ClassId) -> &Class {
        &self.classes[id as usize]
    }

    pub fn n_blocks(&self) -> usize {
        self.n_blocks
    }

    pub fn goal(&self) -> &Goal {
        &self.goal
    }

    /// Non-identity transforms that map this puzzle onto itself.
    pub fn symmetries(&self) -> &[Symmetry] {
        &self.symmetries
    }

    #[inline]
    pub fn cell(&self, x: u8, y: u8) -> Cell {
        y * self.width + x
    }

    #[inline]
    pub fn xy(&self, cell: Cell) -> (u8, u8) {
        (cell % self.width, cell / self.width)
    }

    /// Cells covered by a block of `class` anchored at `anchor`.
    #[inline]
    pub fn block_mask(&self, class: ClassId, anchor: Cell) -> Mask {
        self.classes[class as usize].placements[anchor as usize]
    }

    fn transform_mask(&self, t: Transform, mask: Mask) -> Mask {
        cells(mask).fold(0, |acc, cell| {
            let (x, y) = self.xy(cell);
            let (tx, ty) = t.apply(x, y, self.width, self.height);
            acc | bit(self.cell(tx, ty))
        })
    }

    fn shape_of_mask(&self, mask: Mask) -> Shape {
        Shape::from_cells(cells(mask).map(|c| self.xy(c)))
            .expect("non-empty mask")
            .0
    }

    /// Checks whether `t` maps board, classes and goal onto themselves.
    fn symmetry(&self, t: Transform) -> Option<Symmetry> {
        if t.swaps_axes() && self.width != self.height {
            return None;
        }
        if self.transform_mask(t, self.open) != self.open {
            return None;
        }
        let mut class_perm = Vec::with_capacity(self.classes.len());
        let mut anchor_map = Vec::with_capacity(self.classes.len());
        for class in &self.classes {
            let sample = class.placements.iter().copied().find(|&m| m != 0)?;
            let image = self.shape_of_mask(self.transform_mask(t, sample));
            let axis = if t.swaps_axes() { class.axis.swapped() } else { class.axis };
            let target = self.classes.iter().position(|c| {
                c.shape == image && c.tag == class.tag && c.axis == axis && c.count == class.count
            })?;
            class_perm.push(target as ClassId);

            let mut map = [NO_CELL; MAX_CELLS];
            for (anchor, &mask) in class.placements.iter().enumerate() {
                if mask != 0 {
                    map[anchor] = self.transform_mask(t, mask).trailing_zeros() as Cell;
                }
            }
            anchor_map.push(map);
        }
        let symmetry = Symmetry { transform: t, class_perm, anchor_map };
        let image = self.goal.map(&|class, anchor| symmetry.map(class, anchor));
        (image.normalized() == self.goal.normalized()).then_some(symmetry)
    }
}

fn build_class(def: &ClassDef, width: u8, height: u8, open: Mask) -> Class {
    let (w, h) = (width as i16, height as i16);
    let mut placements = [0; MAX_CELLS];
    for ay in 0..h {
        for ax in 0..w {
            let mut mask = 0;
            let fits = def.shape.offsets().iter().all(|&(dx, dy)| {
                let (x, y) = (ax + dx as i16, ay + dy as i16);
                if x < 0 || x >= w || y < 0 || y >= h {
                    return false;
                }
                let cell = bit((y * w + x) as Cell);
                mask |= cell;
                open & cell != 0
            });
            if fits {
                placements[(ay * w + ax) as usize] = mask;
            }
        }
    }

    let mut steps = [[NO_CELL; MAX_CELLS]; 4];
    for ay in 0..h {
        for ax in 0..w {
            let anchor = (ay * w + ax) as usize;
            if placements[anchor] == 0 {
                continue;
            }
            for dir in Dir::ALL {
                if !def.axis.allows(dir) {
                    continue;
                }
                let (dx, dy) = dir.delta();
                let (x, y) = (ax + dx as i16, ay + dy as i16);
                if x < 0 || x >= w || y < 0 || y >= h {
                    continue;
                }
                let next = (y * w + x) as usize;
                if placements[next] != 0 {
                    steps[dir.index()][anchor] = next as Cell;
                }
            }
        }
    }

    Class {
        shape: def.shape.clone(),
        tag: def.tag.clone(),
        axis: def.axis,
        count: def.count,
        placements,
        steps,
    }
}
