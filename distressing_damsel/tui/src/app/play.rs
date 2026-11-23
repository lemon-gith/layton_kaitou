//! Playing: modes, the cursor, moving blocks, undo and redo, animations.

use std::time::Instant;

use dd_engine::{BlockId, Cell, Dir, Game, Mask};

use super::cursor;
use super::game::GameScreen;
use super::keys::{Action, Context};
use crate::look::{exiting_block, slide_millis, EXIT_MILLIS};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// No cursor.
    Base,
    /// A cursor on a block.
    Select { cursor: BlockId },
    /// A block held, and where it was when it was picked up.
    Grab { cursor: BlockId, start: Cell },
    /// Waiting for a label; where the cursor was, if anywhere.
    Jump { back_to: Option<BlockId> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Animation {
    /// A block sliding along `path` to where the game now has it.
    Slide { block: BlockId, path: Vec<Dir>, started: Instant, millis: u64 },
    /// The solved puzzle's key block fading out.
    Exit { block: BlockId, started: Instant },
}

/// Something the app needs to know about after an action.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Solved,
}

/// The route a block took to reach `now` from `came_from`, with everything
/// else where it is now.
fn path_into(game: &Game, now: Cell, came_from: Cell) -> Vec<Dir> {
    let mut path = game.layout().drag_path(game.position(), now, came_from).unwrap_or_default();
    path.reverse();
    path.iter().map(|d| d.opposite()).collect()
}

impl GameScreen {
    pub fn context(&self) -> Context {
        match self.mode {
            Mode::Base => Context::Game,
            Mode::Select { .. } => Context::Select,
            Mode::Grab { .. } => Context::Grab,
            Mode::Jump { .. } => Context::Jump,
        }
    }

    pub fn cursor(&self) -> Option<BlockId> {
        match self.mode {
            Mode::Select { cursor } | Mode::Grab { cursor, .. } => Some(cursor),
            _ => None,
        }
    }

    pub fn animating(&self) -> bool {
        self.animation.is_some()
    }

    /// Ends an animation that has run its time.
    pub fn tick(&mut self, now: Instant) {
        let done = match &self.animation {
            Some(Animation::Slide { started, millis, .. }) => now.duration_since(*started).as_millis() as u64 >= *millis,
            Some(Animation::Exit { started, .. }) => now.duration_since(*started).as_millis() as u64 >= EXIT_MILLIS,
            None => false,
        };
        if done {
            self.snap();
        }
    }

    /// Jumps any animation to its end.
    pub fn snap(&mut self) {
        if let Some(Animation::Exit { .. }) = self.animation.take() {
            self.exited = true;
        }
    }

    /// Where the cursor starts when selection begins: where it last was,
    /// else on the block the goal is about, else the first block.
    fn default_cursor(&self) -> BlockId {
        self.last_cursor
            .filter(|&b| b < self.game().block_count())
            .or_else(|| {
                let targets = self.game().layout().goal().targets();
                let class = targets.first()?.0;
                self.game().puzzle().blocks.iter().position(|b| b.class == class)
            })
            .unwrap_or(0)
    }

    fn set_mode(&mut self, mode: Mode) {
        if let Some(cursor) = self.cursor() {
            self.last_cursor = Some(cursor);
        }
        self.mode = mode;
    }

    /// Anchors where the held block could be dropped, as the cells it
    /// would cover there.
    pub fn reach(&self) -> Mask {
        let Mode::Grab { cursor, .. } = self.mode else { return 0 };
        let game = self.game();
        let class = game.puzzle().blocks[cursor].class;
        dd_engine::cells(game.reachable(cursor)).fold(0, |acc, anchor| acc | game.layout().block_mask(class, anchor))
    }

    fn slide(&mut self, block: BlockId, path: Vec<Dir>, now: Instant, animate: bool) {
        if animate && !path.is_empty() {
            let millis = slide_millis(path.len());
            self.animation = Some(Animation::Slide { block, path, started: now, millis });
        }
    }

    /// One cell, if the block can go there and the puzzle is not solved.
    fn step(&mut self, block: BlockId, dir: Dir, now: Instant, animate: bool) {
        if self.game().is_solved() {
            return;
        }
        if self.progress.game.step(block, dir).is_ok() {
            self.slide(block, vec![dir], now, animate);
        }
    }

    /// Carries out a game action. Any running animation is finished first,
    /// so keys never wait for it.
    pub fn act(&mut self, action: Action, now: Instant, animate: bool) -> Option<Event> {
        self.snap();
        let was_solved = self.game().is_solved();
        match (self.mode, action) {
            (Mode::Grab { .. }, Action::Undo | Action::Redo) => {}
            (_, Action::Undo) => {
                if let Some(drag) = self.progress.game.undo() {
                    let path = path_into(self.game(), drag.from, drag.to);
                    self.slide(drag.block, path, now, animate);
                }
            }
            (_, Action::Redo) => {
                if let Some(drag) = self.progress.game.redo() {
                    let path = path_into(self.game(), drag.to, drag.from);
                    self.slide(drag.block, path, now, animate);
                }
            }
            (Mode::Base, Action::Select) => self.set_mode(Mode::Select { cursor: self.default_cursor() }),
            (Mode::Base | Mode::Select { .. }, Action::Jump) => self.set_mode(Mode::Jump { back_to: self.cursor() }),
            (Mode::Jump { .. }, Action::Label(label)) => {
                if let Some(block) = self.game().puzzle().blocks.iter().position(|b| b.label == label) {
                    self.set_mode(Mode::Select { cursor: block });
                }
            }
            (Mode::Jump { back_to }, Action::Back) => {
                let cursor = back_to.unwrap_or_else(|| self.default_cursor());
                self.set_mode(Mode::Select { cursor });
            }
            (Mode::Select { cursor }, Action::Move(dir)) => {
                if let Some(next) = cursor::neighbour(self.game(), cursor, dir) {
                    self.set_mode(Mode::Select { cursor: next });
                }
            }
            (Mode::Select { cursor }, Action::Slide(dir)) => self.step(cursor, dir, now, animate),
            (Mode::Select { cursor }, Action::Grab) => {
                if !was_solved {
                    let start = self.game().anchor(cursor).expect("cursor is a block");
                    self.set_mode(Mode::Grab { cursor, start });
                }
            }
            (Mode::Select { .. }, Action::Back) => self.set_mode(Mode::Base),
            (Mode::Grab { cursor, .. }, Action::Slide(dir)) => self.step(cursor, dir, now, animate),
            (Mode::Grab { cursor, .. }, Action::Drop) => self.set_mode(Mode::Select { cursor }),
            (Mode::Grab { cursor, start }, Action::Back) => {
                // Put it back where the grab began. Only this block has
                // moved since, so the way back is always clear.
                let now_at = self.game().anchor(cursor).expect("cursor is a block");
                if now_at != start && self.progress.game.drag_to(cursor, start).is_ok() {
                    let path = path_into(self.game(), start, now_at);
                    self.slide(cursor, path, now, animate);
                }
                self.set_mode(Mode::Select { cursor });
            }
            _ => {}
        }

        let solved = self.game().is_solved();
        if !solved {
            self.exited = false;
        }
        if solved && !was_solved {
            self.set_mode(Mode::Base);
            // Let the last slide finish, then the exit.
            let started = match &self.animation {
                Some(Animation::Slide { started, millis, .. }) => *started + std::time::Duration::from_millis(*millis),
                _ => now,
            };
            match exiting_block(self.game()) {
                Some(block) if animate => {
                    self.pending_exit = Some(Animation::Exit { block, started });
                }
                Some(_) => self.exited = true,
                None => {}
            }
            return Some(Event::Solved);
        }
        None
    }

    /// Moves on from a finished slide to a waiting exit animation.
    pub fn advance(&mut self, now: Instant) {
        self.tick(now);
        if self.animation.is_none() {
            if let Some(exit) = self.pending_exit.take() {
                self.animation = Some(exit);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::home::testing::TempDir;
    use crate::data::library::Source;
    use dd_engine::PuzzleSpec;
    use std::time::Duration;

    fn screen(dir: &std::path::Path, grid: &str, goal: (char, u8, u8)) -> GameScreen {
        let mut spec = PuzzleSpec::from_grid(grid).unwrap();
        spec.goals.push(dd_engine::GoalSpec { block: goal.0, x: goal.1, y: goal.2 });
        GameScreen::open(spec, Source::Code, dir).unwrap().0
    }

    fn klotski(dir: &std::path::Path) -> GameScreen {
        let spec = dd_engine::file::parse(include_str!("../../../puzzles/klotski.toml")).unwrap();
        GameScreen::open(spec, Source::Code, dir).unwrap().0
    }

    fn label(s: &GameScreen, b: BlockId) -> char {
        s.game().puzzle().blocks[b].label
    }

    fn act(s: &mut GameScreen, actions: &[Action]) {
        for &a in actions {
            s.act(a, Instant::now(), false);
        }
    }

    #[test]
    fn modes_step_up_and_esc_steps_down() {
        let tmp = TempDir::new();
        let mut s = klotski(&tmp.0);
        assert_eq!(s.context(), Context::Game);
        act(&mut s, &[Action::Select]);
        // The cursor starts on the key block, D.
        assert_eq!(s.cursor().map(|b| label(&s, b)), Some('D'));
        act(&mut s, &[Action::Jump]);
        assert_eq!(s.context(), Context::Jump);
        act(&mut s, &[Action::Back]);
        assert_eq!(s.cursor().map(|b| label(&s, b)), Some('D'), "Esc from jump returns to where the cursor was");
        act(&mut s, &[Action::Back]);
        assert_eq!(s.context(), Context::Game);

        // Jump straight from base; Esc lands in selection.
        act(&mut s, &[Action::Jump, Action::Label('J')]);
        assert_eq!(s.cursor().map(|b| label(&s, b)), Some('J'));
        act(&mut s, &[Action::Jump, Action::Label('Z')]);
        assert_eq!(s.context(), Context::Jump, "an unknown label keeps waiting");
        act(&mut s, &[Action::Back]);
        assert_eq!(s.cursor().map(|b| label(&s, b)), Some('J'));
    }

    #[test]
    fn arrows_move_the_cursor_and_shift_slides() {
        let tmp = TempDir::new();
        let mut s = klotski(&tmp.0);
        // AABBC / DDEF. / DDEG. / HHIIJ: from D, right twice reaches F.
        act(&mut s, &[Action::Select, Action::Move(Dir::Right), Action::Move(Dir::Right)]);
        assert_eq!(s.cursor().map(|b| label(&s, b)), Some('F'));
        // F cannot go left (E is there); it can go right into the gap.
        act(&mut s, &[Action::Slide(Dir::Left), Action::Slide(Dir::Right)]);
        assert_eq!(s.game().move_count(), 1);
        act(&mut s, &[Action::Slide(Dir::Down)]);
        assert_eq!(s.game().move_count(), 1, "still the same block, still one move");
    }

    #[test]
    fn grab_slide_drop_and_esc_puts_back() {
        let tmp = TempDir::new();
        let mut s = klotski(&tmp.0);
        act(&mut s, &[Action::Jump, Action::Label('G'), Action::Grab]);
        assert_eq!(s.context(), Context::Grab);
        let g = s.cursor().unwrap();
        let start = s.game().anchor(g).unwrap();
        assert_ne!(s.reach(), 0);
        act(&mut s, &[Action::Slide(Dir::Right), Action::Slide(Dir::Up)]);
        assert_eq!(s.game().move_count(), 1);
        act(&mut s, &[Action::Undo]);
        assert_eq!(s.game().move_count(), 1, "undo waits until the block is dropped");
        act(&mut s, &[Action::Back]);
        assert_eq!(s.context(), Context::Select);
        assert_eq!(s.game().anchor(g), Some(start));
        assert_eq!(s.game().move_count(), 0, "putting it back costs nothing");

        act(&mut s, &[Action::Grab, Action::Slide(Dir::Right), Action::Drop]);
        assert_eq!(s.context(), Context::Select);
        assert_eq!(s.game().move_count(), 1);
        assert_eq!(s.reach(), 0);
    }

    #[test]
    fn undo_and_redo_slide_back_along_the_route() {
        let tmp = TempDir::new();
        let mut s = screen(&tmp.0, "A..\n.#.\n...", ('A', 2, 2));
        let a = 0;
        act(&mut s, &[Action::Select]);
        let now = Instant::now();
        s.act(Action::Slide(Dir::Right), now, true);
        s.act(Action::Slide(Dir::Right), now, true);
        assert_eq!(s.game().anchor(a), Some(2));
        s.act(Action::Undo, now, true);
        assert_eq!(s.game().anchor(a), Some(0));
        match &s.animation {
            Some(Animation::Slide { path, .. }) => assert_eq!(path, &[Dir::Left, Dir::Left]),
            other => panic!("expected a slide, got {other:?}"),
        }
        s.act(Action::Redo, now, true);
        match &s.animation {
            Some(Animation::Slide { path, .. }) => assert_eq!(path, &[Dir::Right, Dir::Right]),
            other => panic!("expected a slide, got {other:?}"),
        }
    }

    #[test]
    fn animations_end_on_time_or_on_the_next_key() {
        let tmp = TempDir::new();
        let mut s = klotski(&tmp.0);
        let now = Instant::now();
        act(&mut s, &[Action::Jump, Action::Label('G')]);
        s.act(Action::Slide(Dir::Right), now, true);
        assert!(s.animating());
        s.advance(now + Duration::from_millis(10));
        assert!(s.animating());
        s.advance(now + Duration::from_millis(slide_millis(1)));
        assert!(!s.animating());
        // G is at the right edge now; the cell above it is free.
        s.act(Action::Slide(Dir::Up), now, true);
        assert!(s.animating());
        s.act(Action::Move(Dir::Up), now, true);
        assert!(!s.animating(), "a key finishes the slide");
    }

    #[test]
    fn solving_reports_once_fades_the_key_block_and_freezes_moves() {
        let tmp = TempDir::new();
        let mut s = screen(&tmp.0, "A.\n..", ('A', 1, 1));
        let now = Instant::now();
        act(&mut s, &[Action::Select]);
        assert_eq!(s.act(Action::Slide(Dir::Right), now, true), None);
        assert_eq!(s.act(Action::Slide(Dir::Down), now, true), Some(Event::Solved));
        assert_eq!(s.context(), Context::Game, "back to base mode");
        // The last slide plays, then the exit.
        assert!(matches!(s.animation, Some(Animation::Slide { .. })));
        let after_slide = now + Duration::from_millis(slide_millis(1));
        s.advance(after_slide);
        assert!(matches!(s.animation, Some(Animation::Exit { .. })));
        s.advance(after_slide + Duration::from_millis(EXIT_MILLIS));
        assert!(!s.animating() && s.exited);

        // No more moves; undo still works and brings the block back.
        act(&mut s, &[Action::Select, Action::Slide(Dir::Left)]);
        assert_eq!(s.game().move_count(), 1);
        assert_eq!(s.act(Action::Undo, now, false), None);
        assert!(!s.exited && !s.game().is_solved());
    }
}
