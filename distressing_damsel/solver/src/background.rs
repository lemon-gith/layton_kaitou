//! Runs a [`Solver`] on its own thread so an interface stays responsive.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::{Progress, Solver};

/// Positions expanded between progress reports.
const CHUNK: usize = 4096;

pub struct Handle {
    updates: Receiver<Progress>,
    cancel: Arc<AtomicBool>,
    thread: JoinHandle<Solver>,
    latest: Progress,
}

/// Starts the search. Progress arrives through [`Handle::poll`].
pub fn spawn(mut solver: Solver) -> Handle {
    let latest = solver.progress();
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&cancel);
    let (tx, updates) = mpsc::channel();
    let thread = thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let progress = solver.step(CHUNK);
            if tx.send(progress).is_err() || progress.outcome.is_some() {
                break;
            }
        }
        solver
    });
    Handle { updates, cancel, thread, latest }
}

impl Handle {
    /// The most recent progress report, without blocking.
    pub fn poll(&mut self) -> Progress {
        while let Ok(progress) = self.updates.try_recv() {
            self.latest = progress;
        }
        self.latest
    }

    /// Whether the search thread has stopped, by finishing or by cancelling.
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Asks the search to stop at its next report.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Waits for the thread and hands the solver back, with whatever it
    /// found, for reading out results.
    pub fn join(self) -> Solver {
        self.thread.join().expect("solver thread panicked")
    }
}
