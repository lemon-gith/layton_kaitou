use dd_engine::{file, Game, Metric, Puzzle, PuzzleSpec};
use dd_solver::{Explore, Options, Outcome, Solution, Solver};

const KLOTSKI: &str = include_str!("../../puzzles/klotski.toml");
const ALL: [&str; 5] = [
    KLOTSKI,
    include_str!("../../puzzles/first_steps.toml"),
    include_str!("../../puzzles/gridlock.toml"),
    include_str!("../../puzzles/odd_shapes.toml"),
    include_str!("../../puzzles/trading_places.toml"),
];

fn puzzle(source: &str) -> Puzzle {
    file::parse(source).unwrap().build().unwrap().0
}

fn grid(grid: &str, block: char, x: u8, y: u8) -> Puzzle {
    let mut spec = PuzzleSpec::from_grid(grid).unwrap();
    spec.goals.push(dd_engine::GoalSpec { block, x, y });
    spec.build().unwrap().0
}

fn solved(puzzle: &Puzzle, options: Options) -> Solver {
    let mut solver = Solver::new(puzzle.layout.clone(), puzzle.start, options);
    assert_eq!(solver.run(), Outcome::Solved);
    solver
}

/// Replays a solution through the game engine and checks it wins in exactly
/// as many counted moves as it claims.
fn replay(puzzle: &Puzzle, solution: &Solution) {
    let mut game = Game::new(puzzle.clone());
    for (i, &mv) in solution.moves.iter().enumerate() {
        assert!(!game.is_solved(), "solved early at move {i}");
        game.apply(mv).unwrap();
        assert_eq!(*game.position(), solution.positions[i + 1]);
    }
    assert!(game.is_solved());
    assert_eq!(game.move_count(), solution.len());
}

#[test]
fn klotski_takes_81_moves() {
    let p = puzzle(KLOTSKI);
    let solver = solved(&p, Options::default());
    let solution = solver.solution().unwrap();
    assert_eq!(solution.len(), 81);
    assert_eq!(solver.progress().solution_len, Some(81));
    replay(&p, &solution);
}

#[test]
fn symmetry_changes_the_work_but_not_the_answer() {
    let p = puzzle(KLOTSKI);
    let with = solved(&p, Options::default());
    let without = solved(&p, Options { symmetry: false, ..Options::default() });
    assert_eq!(without.solution().unwrap().len(), 81);
    assert!(with.progress().states * 2 <= without.progress().states + 200);
    replay(&p, &without.solution().unwrap());
}

#[test]
fn klotski_has_25955_reachable_positions() {
    let p = puzzle(KLOTSKI);
    let options = Options {
        symmetry: false,
        explore: Explore::Everything,
        post_solve_factor: f64::INFINITY,
        ..Options::default()
    };
    let solver = solved(&p, options);
    assert!(solver.is_exhaustive());
    assert_eq!(solver.progress().states, 25_955);
    assert_eq!(solver.depth_histogram().iter().sum::<usize>(), 25_955);
}

#[test]
fn klotski_takes_116_single_steps() {
    let p = puzzle(KLOTSKI);
    let solver = solved(&p, Options { metric: Metric::Step, ..Options::default() });
    assert_eq!(solver.solution().unwrap().len(), 116);
}

#[test]
fn every_example_solves_and_replays() {
    for source in ALL {
        let p = puzzle(source);
        for symmetry in [true, false] {
            let solver = solved(&p, Options { symmetry, ..Options::default() });
            let solution = solver.solution().unwrap();
            replay(&p, &solution);
            for alternative in solver.solutions(4) {
                assert_eq!(alternative.len(), solution.len());
                replay(&p, &alternative);
            }
        }
    }
}

#[test]
fn alternatives_are_distinct_and_capped() {
    let p = puzzle(include_str!("../../puzzles/trading_places.toml"));
    let solver = solved(&p, Options::default());
    let solutions = solver.solutions(3);
    assert_eq!(solutions.len(), 3);
    assert_ne!(solutions[0].moves, solutions[1].moves);
    assert_ne!(solutions[1].moves, solutions[2].moves);
    assert!(solver.shortest_count().unwrap() >= 3);

    // A puzzle with one way through offers no alternatives.
    let narrow = grid("A..", 'A', 2, 0);
    let solver = solved(&narrow, Options::default());
    assert_eq!(solver.shortest_count(), Some(1));
    assert_eq!(solver.solutions(5).len(), 1);
}

#[test]
fn shortest_count_matches_brute_force() {
    // Two 1x1 blocks on a 2x2 board, A to the opposite corner. Small enough
    // to count shortest position sequences by exhaustive walk.
    let p = grid("AB\n..", 'A', 1, 1);
    let solver = solved(&p, Options { symmetry: false, ..Options::default() });
    let best = solver.solution().unwrap().len();

    fn count(p: &Puzzle, position: &dd_engine::Position, left: usize) -> u128 {
        if p.layout.is_goal(position) {
            return (left == 0) as u128;
        }
        if left == 0 {
            return 0;
        }
        let mut total = 0;
        p.layout.for_each_successor(position, Metric::Drag, |_, next| {
            total += count(p, next, left - 1);
        });
        total
    }
    assert_eq!(solver.shortest_count(), Some(count(&p, &p.start, best)));
}

#[test]
fn unsolvable_and_capped_searches_say_so() {
    // The wall cuts A off from its target.
    let p = grid("A#.", 'A', 2, 0);
    let mut solver = Solver::new(p.layout.clone(), p.start, Options::default());
    assert_eq!(solver.run(), Outcome::Unsolvable);
    assert!(solver.solution().is_none());
    assert!(solver.solutions(3).is_empty());
    assert_eq!(solver.shortest_count(), None);

    let p = puzzle(KLOTSKI);
    let mut solver = Solver::new(p.layout.clone(), p.start, Options { max_states: 500, ..Options::default() });
    assert_eq!(solver.run(), Outcome::GaveUp);
    assert!(solver.progress().states >= 500);
}

#[test]
fn already_solved_start_needs_no_moves() {
    let p = grid("A.", 'A', 0, 0);
    let solver = solved(&p, Options::default());
    assert_eq!(solver.solution().unwrap().len(), 0);
    assert_eq!(solver.shortest_count(), Some(1));
}

#[test]
fn stepping_in_small_budgets_gives_the_same_result() {
    let p = puzzle(KLOTSKI);
    let mut solver = Solver::new(p.layout.clone(), p.start, Options::default());
    let mut calls = 0;
    let mut last = solver.progress();
    while last.outcome.is_none() {
        let next = solver.step(97);
        assert!(next.states >= last.states && next.depth >= last.depth);
        last = next;
        calls += 1;
    }
    assert!(calls > 10);
    assert_eq!(last.outcome, Some(Outcome::Solved));
    assert_eq!(last.states, solved(&p, Options::default()).progress().states);
}

#[test]
fn post_solve_exploration_is_bounded_by_the_factor() {
    let p = puzzle(KLOTSKI);
    let shortest = solved(&p, Options::default()).progress().states;
    let options = Options { explore: Explore::Everything, post_solve_factor: 1.02, ..Options::default() };
    let wider = solved(&p, options);
    let held = wider.progress().states;
    assert!(held > shortest && !wider.is_exhaustive());
    // One expansion can overshoot the limit by a handful of positions.
    assert!(held < (shortest as f64 * 1.02) as usize + 64);
}

#[test]
fn hints_lead_to_the_goal_by_a_shortest_route() {
    for symmetry in [true, false] {
        let p = puzzle(KLOTSKI);
        let options = Options {
            symmetry,
            explore: Explore::Everything,
            post_solve_factor: f64::INFINITY,
            ..Options::default()
        };
        let solver = solved(&p, options);
        let map = solver.distance_map();
        assert!(map.is_exact());
        assert_eq!(solver.distance(&map, &p.start), Some(81));

        let mut game = Game::new(p);
        let mut expected = 81;
        while let Some(hint) = solver.hint(&map, game.position()) {
            expected -= 1;
            assert_eq!(hint.remaining, expected);
            game.apply(hint.mv).unwrap();
        }
        assert!(game.is_solved());
        assert_eq!(expected, 0);
    }
}

#[cfg(feature = "threads")]
#[test]
fn background_search_reports_and_cancels() {
    let p = puzzle(KLOTSKI);
    let mut handle = dd_solver::background::spawn(Solver::new(p.layout.clone(), p.start, Options::default()));
    let solver = loop {
        let progress = handle.poll();
        if progress.outcome.is_some() {
            break handle.join();
        }
        std::thread::yield_now();
    };
    assert_eq!(solver.solution().unwrap().len(), 81);

    // A search too big to finish quickly stops when asked.
    let options = Options {
        symmetry: false,
        explore: Explore::Everything,
        post_solve_factor: f64::INFINITY,
        ..Options::default()
    };
    let handle = dd_solver::background::spawn(Solver::new(p.layout.clone(), p.start, options));
    handle.cancel();
    let solver = handle.join();
    assert!(solver.progress().states <= 25_955);
}
