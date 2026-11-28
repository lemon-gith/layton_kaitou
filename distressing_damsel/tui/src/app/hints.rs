//! The background solver's state on the game screen, and hints.

use std::time::Instant;

use dd_engine::{BlockId, Cell, Move, Position};
use dd_solver::{DistanceMap, Explore, Options, Outcome, Solver};

use super::game::GameScreen;
use super::solving::Job;

/// How long a new hint's block pulses, in milliseconds: two pulses.
pub const PULSE_MILLIS: u64 = 1200;

pub enum SolverState {
    /// Searching from the puzzle's start.
    Running(Job),
    /// Solved; the map gives distances for hints.
    Solved { solver: Box<Solver>, map: DistanceMap },
    NoSolution,
    GaveUp,
}

impl SolverState {
    /// Starts solving `game`'s puzzle from its starting position, mapping
    /// what it can for hints from wherever the player gets to.
    pub fn start(game: &dd_engine::Game) -> SolverState {
        let puzzle = game.puzzle();
        let options = Options { explore: Explore::Everything, ..Options::default() };
        SolverState::Running(Job::start(Solver::new(puzzle.layout.clone(), puzzle.start, options), true))
    }

    pub fn shortest(&self) -> Option<u32> {
        match self {
            SolverState::Solved { solver, .. } => solver.progress().solution_len,
            _ => None,
        }
    }
}

/// A hint on the board: the block to move and where to.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ShownHint {
    pub block: BlockId,
    pub to: Cell,
    pub started: Instant,
    /// The block has moved since the hint appeared.
    pub moved: bool,
}

/// A search for a hint from a position the main search never reached.
pub struct HintSearch {
    pub job: Job,
    /// The position it was asked from; a result for any other is dropped.
    pub from: Position,
}

impl GameScreen {
    /// Whether a hint can be given now, or why not.
    pub fn hint_ready(&self) -> Result<(), &'static str> {
        if self.game().is_solved() {
            return Err("already solved");
        }
        if self.hint_search.is_some() {
            return Err("already finding a hint");
        }
        match self.solver {
            SolverState::Running(_) => Err("the solver is still working; hints are ready when it finishes"),
            SolverState::NoSolution => Err("no hints: this puzzle has no solution"),
            SolverState::GaveUp => Err("no hints: the solver gave up on this puzzle"),
            SolverState::Solved { .. } => Ok(()),
        }
    }

    /// Gives a hint: from the solver's map if it knows this position,
    /// otherwise by a fresh search from here, shown when it finishes.
    pub fn give_hint(&mut self, now: Instant) {
        if self.hint_ready().is_err() {
            return;
        }
        let SolverState::Solved { solver, map } = &self.solver else { return };
        let position = *self.game().position();
        match solver.hint(map, &position) {
            Some(hint) => self.show_hint(hint.mv, now),
            None => {
                let layout = self.game().puzzle().layout.clone();
                let job = Job::start(Solver::new(layout, position, Options::default()), false);
                self.hint_search = Some(HintSearch { job, from: position });
            }
        }
    }

    fn show_hint(&mut self, mv: Move, now: Instant) {
        let game = self.game();
        let block = (0..game.block_count())
            .find(|&b| game.anchor(b) == Some(mv.from) && game.puzzle().blocks[b].class == mv.class);
        if let Some(block) = block {
            self.hint = Some(ShownHint { block, to: mv.to, started: now, moved: false });
            self.progress.counters.hints += 1;
        }
    }

    /// Collects finished searches.
    pub fn poll_solver(&mut self, now: Instant) {
        if let SolverState::Running(job) = &mut self.solver {
            if let Some(done) = job.poll() {
                self.solver = match (done.outcome, done.map) {
                    (Outcome::Solved, Some(map)) => SolverState::Solved { solver: Box::new(done.solver), map },
                    (Outcome::Unsolvable, _) => SolverState::NoSolution,
                    _ => SolverState::GaveUp,
                };
            }
        }
        if let Some(search) = &mut self.hint_search {
            if let Some(done) = search.job.poll() {
                let from = search.from;
                self.hint_search = None;
                let first = done.solver.solution().and_then(|s| s.moves.first().copied());
                if let (Some(mv), true) = (first, *self.game().position() == from) {
                    self.show_hint(mv, now);
                }
            }
        }
    }

    /// Whether anything is in progress that the screen should keep
    /// redrawing for: a search, or a hint pulsing.
    pub fn busy(&self, now: Instant) -> bool {
        matches!(self.solver, SolverState::Running(_))
            || self.hint_search.is_some()
            || self.replay.is_some()
            || self.hint.is_some_and(|h| now.saturating_duration_since(h.started).as_millis() < PULSE_MILLIS as u128)
    }

    /// How bright the hinted block is at `now`: two pulses, then steady.
    pub fn hint_pulse(&self, now: Instant) -> Option<(BlockId, f32)> {
        let hint = self.hint?;
        let t = now.saturating_duration_since(hint.started).as_millis() as f32 / PULSE_MILLIS as f32;
        (t < 1.0).then(|| (hint.block, (t * 2.0 * std::f32::consts::PI).sin().powi(2)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keys::Action;
    use crate::data::home::testing::TempDir;
    use crate::data::library::Source;
    use dd_engine::{Dir, PuzzleSpec};

    fn screen(dir: &std::path::Path, source: &str) -> GameScreen {
        let spec = dd_engine::file::parse(source).unwrap();
        GameScreen::open(spec, Source::Code, dir).unwrap().0
    }

    /// Waits for the background search, as the app's tick would.
    fn settle(s: &mut GameScreen) {
        for _ in 0..2000 {
            s.poll_solver(Instant::now());
            if !matches!(s.solver, SolverState::Running(_)) && s.hint_search.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("solver did not finish");
    }

    fn label(s: &GameScreen, b: BlockId) -> char {
        s.game().puzzle().blocks[b].label
    }

    #[test]
    fn hints_wait_for_the_solver_then_lead_to_the_goal() {
        let tmp = TempDir::new();
        let mut s = screen(&tmp.0, include_str!("../../../puzzles/first_steps.toml"));
        // Until the app collects the finished search, there are no hints.
        assert!(s.hint_ready().is_err());
        settle(&mut s);
        assert_eq!(s.solver.shortest(), Some(6));
        assert_eq!(s.hint_ready(), Ok(()));

        // Following every hint solves the puzzle in the shortest number of
        // moves, counting each hint.
        let now = Instant::now();
        while !s.game().is_solved() {
            s.give_hint(now);
            settle(&mut s);
            let hint = s.hint.expect("a hint");
            let to = hint.to;
            s.progress.game.drag_to(hint.block, to).unwrap();
            s.act(Action::Select, now, false); // any action runs the clearing rules
            assert!(s.hint.is_none(), "reaching the ghost clears it");
            s.act(Action::Back, now, false);
        }
        assert_eq!(s.game().move_count(), 6);
        assert_eq!(s.progress.counters.hints, 6);
        assert_eq!(s.hint_ready(), Err("already solved"));
    }

    #[test]
    fn hints_from_unexplored_positions_come_from_a_fresh_search() {
        let tmp = TempDir::new();
        let mut s = screen(&tmp.0, include_str!("../../../puzzles/klotski.toml"));
        settle(&mut s);
        // Swap in a search that has only seen the start: every other
        // position is unknown to it, as on a puzzle too big to map.
        let puzzle = s.game().puzzle();
        let tiny = Solver::new(puzzle.layout.clone(), puzzle.start, Options::default());
        let map = tiny.distance_map();
        s.solver = SolverState::Solved { solver: Box::new(tiny), map };
        s.give_hint(Instant::now());
        assert!(s.hint_search.is_some() && s.hint.is_none());
        assert_eq!(s.hint_ready(), Err("already finding a hint"));
        settle(&mut s);
        assert!(s.hint.is_some());
        assert_eq!(s.progress.counters.hints, 1);
    }

    #[test]
    fn a_hint_for_an_old_position_is_dropped() {
        let tmp = TempDir::new();
        let mut s = screen(&tmp.0, include_str!("../../../puzzles/first_steps.toml"));
        settle(&mut s);
        let from = *s.game().position();
        let layout = s.game().puzzle().layout.clone();
        s.hint_search = Some(HintSearch { job: Job::start(Solver::new(layout, from, Options::default()), false), from });
        // The player moves D before the search finishes.
        s.act(Action::Jump, Instant::now(), false);
        s.act(Action::Label('D'), Instant::now(), false);
        s.act(Action::Slide(Dir::Right), Instant::now(), false);
        assert_ne!(*s.game().position(), from);
        settle(&mut s);
        assert!(s.hint.is_none());
        assert_eq!(s.progress.counters.hints, 0);
    }

    #[test]
    fn the_ghost_clears_when_another_block_moves_or_on_undo() {
        let tmp = TempDir::new();
        let mut spec = PuzzleSpec::from_grid("A...\n....\nB...").unwrap();
        spec.goals.push(dd_engine::GoalSpec { block: 'A', x: 3, y: 0 });
        let mut s = GameScreen::open(spec, Source::Code, &tmp.0).unwrap().0;
        settle(&mut s);
        s.give_hint(Instant::now());
        let hint = s.hint.unwrap();
        assert_eq!(label(&s, hint.block), 'A');

        // Another block moving clears it.
        s.act(Action::Jump, Instant::now(), false);
        s.act(Action::Label('B'), Instant::now(), false);
        s.act(Action::Slide(Dir::Right), Instant::now(), false);
        assert!(s.hint.is_none());

        // Moving the hinted block without reaching the ghost keeps it;
        // undo clears it.
        s.give_hint(Instant::now());
        assert_eq!(label(&s, s.hint.unwrap().block), 'A');
        s.act(Action::Jump, Instant::now(), false);
        s.act(Action::Label('A'), Instant::now(), false);
        s.act(Action::Slide(Dir::Down), Instant::now(), false);
        assert!(s.hint.is_some_and(|h| h.moved));
        s.act(Action::Undo, Instant::now(), false);
        assert!(s.hint.is_none());
    }
}
