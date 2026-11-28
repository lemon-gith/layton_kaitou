//! Replaying the player's own solution, move by move.

use std::time::{Duration, Instant};

use dd_engine::{Drag, Game, Puzzle};

use super::play::{path_into, Animation};
use crate::look::{exiting_block, slide_millis, EXIT_MILLIS};

/// Pause before the first move and between moves, in milliseconds.
const LEAD_IN: u64 = 600;
const GAP: u64 = 250;
/// Time per move when animations are off.
const STEP_WITHOUT_ANIMATION: u64 = 450;

pub struct Replay {
    /// The board as the replay has reached it.
    pub game: Game,
    moves: Vec<Drag>,
    next: usize,
    pub animation: Option<Animation>,
    ready_at: Instant,
    animate: bool,
    /// The key block has faded out at the end.
    pub exited: bool,
    pub finished: bool,
}

impl Replay {
    pub fn new(puzzle: Puzzle, moves: Vec<Drag>, now: Instant, animate: bool) -> Replay {
        Replay {
            game: Game::new(puzzle),
            moves,
            next: 0,
            animation: None,
            ready_at: now + Duration::from_millis(LEAD_IN),
            animate,
            exited: false,
            finished: false,
        }
    }

    /// Moves shown so far, and how many there are.
    pub fn position(&self) -> (usize, usize) {
        (self.next, self.moves.len())
    }

    pub fn advance(&mut self, now: Instant) {
        let elapsed = |started: Instant| now.saturating_duration_since(started).as_millis() as u64;
        match self.animation {
            Some(Animation::Slide { started, millis, .. }) if elapsed(started) >= millis => self.animation = None,
            Some(Animation::Exit { started, .. }) if elapsed(started) >= EXIT_MILLIS => {
                self.animation = None;
                self.exited = true;
            }
            _ => {}
        }
        if self.animation.is_some() || now < self.ready_at || self.finished {
            return;
        }
        if let Some(&drag) = self.moves.get(self.next) {
            self.next += 1;
            if self.game.drag_to(drag.block, drag.to).is_err() {
                // A saved history is checked on load, so this cannot happen;
                // stop rather than show nonsense if it somehow does.
                self.finished = true;
                return;
            }
            let path = path_into(&self.game, drag.to, drag.from);
            let millis = if self.animate { slide_millis(path.len()) } else { STEP_WITHOUT_ANIMATION };
            if self.animate {
                self.animation = Some(Animation::Slide { block: drag.block, path, started: now, millis });
            }
            self.ready_at = now + Duration::from_millis(millis + GAP);
        } else if !self.exited {
            match exiting_block(&self.game) {
                Some(block) if self.animate => self.animation = Some(Animation::Exit { block, started: now }),
                _ => self.exited = true,
            }
        } else {
            self.finished = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_engine::{Dir, PuzzleSpec};

    fn solved() -> Game {
        let mut spec = PuzzleSpec::from_grid("A..\n...").unwrap();
        // A goes right then down, one move, onto its target.
        spec.goals.push(dd_engine::GoalSpec { block: 'A', x: 1, y: 1 });
        let mut game = Game::new(spec.build().unwrap().0);
        game.step(0, Dir::Right).unwrap();
        game.step(0, Dir::Down).unwrap();
        game
    }

    fn run(replay: &mut Replay, from: Instant) -> Instant {
        let mut now = from;
        for _ in 0..1000 {
            replay.advance(now);
            if replay.finished {
                return now;
            }
            now += Duration::from_millis(50);
        }
        panic!("replay did not finish");
    }

    #[test]
    fn plays_every_move_then_the_exit_then_finishes() {
        let game = solved();
        let start = Instant::now();
        let mut replay = Replay::new(game.puzzle().clone(), game.history().to_vec(), start, true);
        assert_eq!(replay.position(), (0, 1));
        replay.advance(start);
        assert_eq!(replay.position(), (0, 1), "waits a moment before the first move");
        replay.advance(start + Duration::from_millis(LEAD_IN));
        assert_eq!(replay.position(), (1, 1));
        assert!(matches!(replay.animation, Some(Animation::Slide { ref path, .. }) if path == &[Dir::Right, Dir::Down]));
        let end = run(&mut replay, start);
        assert!(replay.exited && replay.game.is_solved());
        assert!(end > start + Duration::from_millis(LEAD_IN + EXIT_MILLIS));
    }

    #[test]
    fn without_animations_moves_still_come_one_at_a_time() {
        let game = solved();
        let start = Instant::now();
        let mut replay = Replay::new(game.puzzle().clone(), game.history().to_vec(), start, false);
        run(&mut replay, start);
        assert!(replay.animation.is_none());
        assert!(replay.exited && replay.game.is_solved());
    }
}
