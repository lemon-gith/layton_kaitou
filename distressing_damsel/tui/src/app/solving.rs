//! Running the solver in the background while the player plays.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use dd_solver::{DistanceMap, Outcome, Progress, Solver};

/// Positions expanded between progress reports.
const CHUNK: usize = 2048;

/// A finished search, and the distances for hints if they were wanted.
pub struct Finished {
    pub solver: Solver,
    pub outcome: Outcome,
    pub map: Option<DistanceMap>,
}

enum Update {
    Progress(Progress),
    Done(Box<Finished>),
}

/// A search running on its own thread. Dropping the job stops it.
pub struct Job {
    cancel: Arc<AtomicBool>,
    updates: Receiver<Update>,
    latest: Progress,
}

impl Job {
    /// Starts `solver`. With `with_map`, a solved search also builds the
    /// distance map, on the same thread.
    pub fn start(mut solver: Solver, with_map: bool) -> Job {
        let latest = solver.progress();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&cancel);
        let (tx, updates) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let progress = solver.step(CHUNK);
                if let Some(outcome) = progress.outcome {
                    let map = (with_map && outcome == Outcome::Solved).then(|| solver.distance_map());
                    let _ = tx.send(Update::Done(Box::new(Finished { solver, outcome, map })));
                    return;
                }
                if tx.send(Update::Progress(progress)).is_err() {
                    return;
                }
            }
        });
        Job { cancel, updates, latest }
    }

    /// The latest progress, and the result once the search has finished.
    pub fn poll(&mut self) -> Option<Box<Finished>> {
        while let Ok(update) = self.updates.try_recv() {
            match update {
                Update::Progress(p) => self.latest = p,
                Update::Done(finished) => return Some(finished),
            }
        }
        None
    }

    pub fn progress(&self) -> Progress {
        self.latest
    }

    /// Blocks until the search finishes. For tests.
    #[cfg(test)]
    pub fn wait(mut self) -> Box<Finished> {
        loop {
            match self.updates.recv().expect("solver thread ended early") {
                Update::Progress(p) => self.latest = p,
                Update::Done(finished) => return finished,
            }
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_solver::{Explore, Options};

    fn solver(source: &str, explore: Explore) -> Solver {
        let puzzle = dd_engine::file::parse(source).unwrap().build().unwrap().0;
        Solver::new(puzzle.layout, puzzle.start, Options { explore, ..Options::default() })
    }

    #[test]
    fn solves_in_the_background_with_a_map() {
        let job = Job::start(solver(include_str!("../../../puzzles/klotski.toml"), Explore::Everything), true);
        let done = job.wait();
        assert_eq!(done.outcome, Outcome::Solved);
        assert_eq!(done.solver.progress().solution_len, Some(81));
        assert!(done.map.is_some_and(|m| m.is_exact()));
    }

    #[test]
    fn polling_eventually_finishes_and_dropping_stops() {
        let mut job = Job::start(solver(include_str!("../../../puzzles/first_steps.toml"), Explore::ShortestOnly), false);
        let done = loop {
            if let Some(done) = job.poll() {
                break done;
            }
            std::thread::yield_now();
        };
        assert_eq!(done.outcome, Outcome::Solved);
        assert!(done.map.is_none());

        // A job dropped part-way stops without complaint.
        drop(Job::start(solver(include_str!("../../../puzzles/klotski.toml"), Explore::Everything), true));
    }
}
