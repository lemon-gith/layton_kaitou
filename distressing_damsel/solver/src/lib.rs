//! Shortest-solution search for sliding-block puzzles.
//!
//! [`Solver`] is a breadth-first search you drive yourself: call
//! [`Solver::step`] with a work budget until it reports an outcome. That
//! keeps this crate free of threads and clocks, so the same code runs in a
//! terminal app (see [`background`]) or in a browser worker.
//!
//! States are stored as packed position codes, numbered in the order they
//! are found. Breadth-first order means numbers increase with depth, so a
//! state's depth comes from a short table of where each layer starts rather
//! than from per-state storage.

mod analysis;
#[cfg(feature = "threads")]
pub mod background;
mod store;

use std::sync::Arc;

use dd_engine::{Layout, Metric, Position};

pub use analysis::{DistanceMap, Hint, Solution};
use store::Store;

/// How far to search once the goal has been reached.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Explore {
    /// Stop as soon as every shortest solution has been found.
    #[default]
    ShortestOnly,
    /// Keep going to map the rest of the reachable positions, which makes
    /// hints from any position possible. Bounded by
    /// [`Options::post_solve_factor`].
    Everything,
}

#[derive(Copy, Clone, Debug)]
pub struct Options {
    /// Give up once this many positions have been stored.
    pub max_states: usize,
    pub explore: Explore,
    /// With [`Explore::Everything`], stop once the search holds this many
    /// times the positions it held when the shortest solutions were complete.
    pub post_solve_factor: f64,
    /// Treat mirror-image positions as one where the puzzle allows it.
    pub symmetry: bool,
    /// What counts as one move.
    pub metric: Metric,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            max_states: 5_000_000,
            explore: Explore::ShortestOnly,
            post_solve_factor: 2.0,
            symmetry: true,
            metric: Metric::Drag,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Solved,
    /// Every reachable position was examined and none meets the goal.
    Unsolvable,
    /// [`Options::max_states`] was hit before a solution was found.
    GaveUp,
}

/// Snapshot of a search.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    /// Positions stored so far.
    pub states: usize,
    /// Positions whose moves have been generated.
    pub expanded: usize,
    /// Depth currently being expanded.
    pub depth: u32,
    /// Length of the shortest solution, once known.
    pub solution_len: Option<u32>,
    pub outcome: Option<Outcome>,
}

pub struct Solver {
    layout: Arc<Layout>,
    start: Position,
    options: Options,
    store: Store,
    /// `layers[d]` is the number of the first state at depth `d`.
    layers: Vec<u32>,
    cursor: u32,
    layer_end: u32,
    goal_depth: Option<u32>,
    /// Goal states at the shortest depth.
    goals: Vec<u32>,
    /// Every goal state found.
    all_goals: Vec<u32>,
    states_at_solve: Option<usize>,
    exhausted: bool,
    outcome: Option<Outcome>,
    key: Vec<u8>,
    scratch: Vec<u8>,
}

impl Solver {
    pub fn new(layout: Arc<Layout>, start: Position, options: Options) -> Solver {
        let mut solver = Solver {
            store: Store::new(layout.code_len()),
            layout,
            start,
            options,
            layers: vec![0],
            cursor: 0,
            layer_end: 1,
            goal_depth: None,
            goals: Vec::new(),
            all_goals: Vec::new(),
            states_at_solve: None,
            exhausted: false,
            outcome: None,
            key: Vec::new(),
            scratch: Vec::new(),
        };
        let mut key = Vec::new();
        solver.encode(&start, &mut key);
        solver.store.insert(&key, 0);
        if solver.layout.is_goal(&start) {
            solver.goal_depth = Some(0);
            solver.goals.push(0);
            solver.all_goals.push(0);
            solver.states_at_solve = Some(1);
            if options.explore == Explore::ShortestOnly {
                solver.outcome = Some(Outcome::Solved);
            }
        }
        solver
    }

    pub fn layout(&self) -> &Arc<Layout> {
        &self.layout
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Writes the key for `position` into `out`.
    fn encode(&self, position: &Position, out: &mut Vec<u8>) {
        if self.options.symmetry {
            let mut scratch = Vec::new();
            self.layout.encode_canonical_into(position, out, &mut scratch);
        } else {
            self.layout.encode_into(position, out);
        }
    }

    /// Generates moves for up to `budget` positions and reports where the
    /// search stands. Call until `outcome` is set.
    pub fn step(&mut self, budget: usize) -> Progress {
        let mut left = budget;
        while self.outcome.is_none() && left > 0 {
            if self.cursor == self.layer_end && !self.next_layer() {
                break;
            }
            self.expand(self.cursor);
            self.cursor += 1;
            left -= 1;

            let held = self.store.len();
            let limit = self
                .states_at_solve
                .map(|n| (n as f64 * self.options.post_solve_factor) as usize);
            if held >= self.options.max_states.min(u32::MAX as usize - 1) {
                self.outcome = Some(match self.goal_depth {
                    Some(_) => Outcome::Solved,
                    None => Outcome::GaveUp,
                });
            } else if limit.is_some_and(|limit| held >= limit) {
                self.outcome = Some(Outcome::Solved);
            }
        }
        self.progress()
    }

    /// Called when a layer has been fully expanded. Returns `false` if the
    /// search is over.
    fn next_layer(&mut self) -> bool {
        let finished = self.layers.len() as u32 - 1;
        if self.goal_depth == Some(finished + 1) && self.states_at_solve.is_none() {
            self.states_at_solve = Some(self.store.len());
            if self.options.explore == Explore::ShortestOnly {
                self.outcome = Some(Outcome::Solved);
                return false;
            }
        }
        if self.store.len() as u32 == self.cursor {
            self.exhausted = true;
            self.outcome = Some(match self.goal_depth {
                Some(_) => Outcome::Solved,
                None => Outcome::Unsolvable,
            });
            return false;
        }
        self.layers.push(self.cursor);
        self.layer_end = self.store.len() as u32;
        true
    }

    fn expand(&mut self, id: u32) {
        let layout = Arc::clone(&self.layout);
        let position = layout.decode(self.store.key(id)).expect("stored keys decode");
        let child_depth = self.layers.len() as u32;
        let symmetry = self.options.symmetry;
        let Solver { store, key, scratch, goal_depth, goals, all_goals, .. } = self;
        layout.for_each_successor(&position, self.options.metric, |_, next| {
            if symmetry {
                layout.encode_canonical_into(next, key, scratch);
            } else {
                layout.encode_into(next, key);
            }
            let (child, new) = store.insert(key, id);
            if new && layout.is_goal(next) {
                all_goals.push(child);
                if *goal_depth.get_or_insert(child_depth) == child_depth {
                    goals.push(child);
                }
            }
        });
    }

    pub fn progress(&self) -> Progress {
        Progress {
            states: self.store.len(),
            expanded: self.cursor as usize,
            depth: self.layers.len() as u32 - 1,
            solution_len: self.goal_depth,
            outcome: self.outcome,
        }
    }

    /// Runs the search to its outcome.
    pub fn run(&mut self) -> Outcome {
        loop {
            if let Some(outcome) = self.step(1 << 16).outcome {
                return outcome;
            }
        }
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }

    /// Whether every reachable position has been stored.
    pub fn is_exhaustive(&self) -> bool {
        self.exhausted
    }

    /// Whether every goal position at the shortest depth has been found, so
    /// that [`Solver::shortest_count`] and [`Solver::solutions`] see them all.
    pub fn shortest_is_complete(&self) -> bool {
        self.states_at_solve.is_some()
    }

    /// Positions found at each depth. The last entry may be partial if the
    /// search stopped early.
    pub fn depth_histogram(&self) -> Vec<usize> {
        let ends = self.layers[1..].iter().copied().chain([self.store.len() as u32]);
        let mut sizes: Vec<usize> = self
            .layers
            .iter()
            .zip(ends)
            .map(|(&start, end)| (end - start) as usize)
            .collect();
        // States past the current layer's end belong to the next depth.
        if self.store.len() as u32 > self.layer_end {
            let last = sizes.len() - 1;
            let spill = self.store.len() - self.layer_end as usize;
            sizes[last] -= spill;
            sizes.push(spill);
        }
        sizes
    }

    fn depth_of(&self, id: u32) -> u32 {
        if id >= self.layer_end {
            return self.layers.len() as u32;
        }
        self.layers.partition_point(|&start| start <= id) as u32 - 1
    }
}
