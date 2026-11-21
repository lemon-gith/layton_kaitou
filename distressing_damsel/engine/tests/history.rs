//! Saving, restoring and comparing move histories.

use dd_engine::{file, Dir, Drag, Game, HistoryError, MoveRecord, Puzzle, PuzzleSpec};

const KLOTSKI: &str = include_str!("../../puzzles/klotski.toml");

fn puzzle(source: &str) -> Puzzle {
    file::parse(source).unwrap().build().unwrap().0
}

fn grid(grid: &str, goal: (char, u8, u8)) -> Puzzle {
    let mut spec = PuzzleSpec::from_grid(grid).unwrap();
    spec.goals.push(dd_engine::GoalSpec { block: goal.0, x: goal.1, y: goal.2 });
    spec.build().unwrap().0
}

fn block(game: &Game, label: char) -> usize {
    game.puzzle().blocks.iter().position(|b| b.label == label).unwrap()
}

/// A few moves of Klotski, made the way a player would: steps that
/// coalesce, a detour that cancels out, then more moves.
fn played() -> Game {
    let mut game = Game::new(puzzle(KLOTSKI));
    let (g, j, f) = (block(&game, 'G'), block(&game, 'J'), block(&game, 'F'));
    game.step(g, Dir::Down).unwrap();
    game.step(j, Dir::Left).unwrap();
    game.step(j, Dir::Right).unwrap(); // back where it was: no move
    game.step(j, Dir::Left).unwrap();
    game.step(f, Dir::Down).unwrap();
    game
}

#[test]
fn a_history_replays_to_the_same_game() {
    let game = played();
    assert_eq!(game.move_count(), 3);
    let replayed = Game::from_history(game.puzzle().clone(), game.history()).unwrap();
    assert_eq!(replayed.position(), game.position());
    assert_eq!(replayed.history(), game.history());
    for b in 0..game.block_count() {
        assert_eq!(replayed.anchor(b), game.anchor(b));
    }
}

#[test]
fn records_round_trip_through_toml() {
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Save {
        moves: Vec<MoveRecord>,
    }
    let game = played();
    let save = Save { moves: game.records() };
    assert_eq!(save.moves[0], MoveRecord { block: 'G', from: [1, 3], to: [1, 4] });
    let text = toml::to_string(&save).unwrap();
    let back: Save = toml::from_str(&text).unwrap();
    assert_eq!(back, save);
    let restored = Game::from_records(game.puzzle().clone(), &back.moves).unwrap();
    assert_eq!(restored.history(), game.history());
}

#[test]
fn bad_histories_are_rejected() {
    let p = puzzle(KLOTSKI);
    let game = played();
    let h = game.history().to_vec();
    let check = |history: &[Drag], expected: HistoryError| {
        assert_eq!(Game::from_history(p.clone(), history).unwrap_err(), expected);
    };

    let mut wrong_start = h.clone();
    wrong_start[1].from = wrong_start[1].to;
    check(&wrong_start, HistoryError::WrongStart { index: 1 });

    let mut empty = h.clone();
    empty[0].to = empty[0].from;
    check(&empty, HistoryError::Empty { index: 0 });

    // G down, then G again: should have been one move.
    let g = h[0];
    let again = Drag { block: g.block, from: g.to, to: g.from };
    check(&[g, again], HistoryError::Uncoalesced { index: 1 });

    // The big square cannot move at the start.
    let b = block(&game, 'B');
    let anchor = p.blocks[b].start;
    check(&[Drag { block: b, from: anchor, to: anchor + 4 }], HistoryError::Illegal { index: 0 });

    let off = [MoveRecord { block: 'G', from: [1, 3], to: [9, 9] }];
    assert_eq!(Game::from_records(p.clone(), &off).unwrap_err(), HistoryError::OffBoard { index: 0 });
    let unknown = [MoveRecord { block: 'Z', from: [0, 0], to: [0, 1] }];
    assert_eq!(Game::from_records(p.clone(), &unknown).unwrap_err(), HistoryError::UnknownBlock('Z'));
}

#[test]
fn swapping_identical_blocks_keeps_the_position_code() {
    // A and B are identical 1x1 blocks; K is the key.
    let p = grid("AB.\n...\nK..", ('K', 2, 2));
    let (mut left, mut right) = (Game::new(p.clone()), Game::new(p));
    let (a, b) = (block(&left, 'A'), block(&left, 'B'));
    // Left: B steps right, then A steps right. A ends in the middle.
    left.step(b, Dir::Right).unwrap();
    left.step(a, Dir::Right).unwrap();
    // Right: A goes round under B to the top right. B stays in the middle.
    right.drag_to(a, right.layout().cell(2, 0)).unwrap();
    assert_eq!(left.anchor(a), Some(1));
    assert_eq!(right.anchor(a), Some(2));
    assert_eq!(left.position_code(), right.position_code());
    assert_eq!(left.position(), right.position());
}

#[test]
fn routes_compare_by_positions_not_by_order() {
    // The same two moves in the other order reach the same position by a
    // different route.
    let p = grid("A..\n...\nK..", ('K', 2, 2));
    let (mut one, mut two) = (Game::new(p.clone()), Game::new(p));
    let (a, k) = (block(&one, 'A'), block(&one, 'K'));
    one.step(a, Dir::Right).unwrap();
    one.step(k, Dir::Right).unwrap();
    two.step(k, Dir::Right).unwrap();
    two.step(a, Dir::Right).unwrap();
    assert_eq!(one.position_code(), two.position_code());
    assert_ne!(one.route(), two.route());
    assert_eq!(one.route().len(), 3);
    assert_eq!(one.route().first(), two.route().first());
    assert_eq!(one.route().last(), Some(&one.position_code()));
}

#[test]
fn set_history_takes_another_route_to_the_same_position() {
    let p = grid("A..\n...\nK..", ('K', 2, 2));
    let (mut long, mut short) = (Game::new(p.clone()), Game::new(p));
    let (a, k) = (block(&long, 'A'), block(&long, 'K'));
    // Long: A right, K right, A further right. Short: K right, A right twice.
    long.step(a, Dir::Right).unwrap();
    long.step(k, Dir::Right).unwrap();
    long.step(a, Dir::Right).unwrap();
    short.step(k, Dir::Right).unwrap();
    short.step(a, Dir::Right).unwrap();
    short.step(a, Dir::Right).unwrap();
    assert_eq!(long.move_count(), 3);
    assert_eq!(short.move_count(), 2);
    assert_eq!(long.position(), short.position());

    long.set_history(short.history()).unwrap();
    assert_eq!(long.move_count(), 2);
    assert_eq!(long.route(), short.route());
    // Undo follows the new route.
    long.undo().unwrap();
    assert_eq!(long.anchor(a), Some(0));

    // A route to somewhere else is refused and changes nothing.
    let before = long.history().to_vec();
    let elsewhere = vec![Drag { block: a, from: 0, to: 1 }];
    assert_eq!(long.set_history(&elsewhere).unwrap_err(), HistoryError::DifferentPosition);
    assert_eq!(long.history(), &before[..]);
}

#[test]
fn set_history_can_trade_identical_blocks() {
    // A and B are identical. This three-move route ends with the starting
    // arrangement, but with A and B having traded places.
    let p = grid("A.B\n...\nK..", ('K', 2, 2));
    let mut swapped = Game::new(p.clone());
    let (a, b) = (block(&swapped, 'A'), block(&swapped, 'B'));
    swapped.drag_to(a, swapped.layout().cell(1, 1)).unwrap();
    swapped.drag_to(b, swapped.layout().cell(0, 0)).unwrap();
    swapped.drag_to(a, swapped.layout().cell(2, 0)).unwrap();
    assert_eq!(swapped.position(), &p.start);

    // A fresh game at the start can adopt that route; the labels follow it.
    let mut fresh = Game::new(p);
    assert_eq!(fresh.anchor(a), Some(0));
    fresh.set_history(swapped.history()).unwrap();
    assert_eq!(fresh.anchor(a), Some(2));
    assert_eq!(fresh.anchor(b), Some(0));
    assert_eq!(fresh.move_count(), 3);
}
