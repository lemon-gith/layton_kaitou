use dd_engine::{
    bit, file, Axis, CellSpec, Dir, Game, Metric, Move, Puzzle, PuzzleSpec, SpecError, Transform, Warning,
};

const KLOTSKI: &str = include_str!("../../puzzles/klotski.toml");
const GRIDLOCK: &str = include_str!("../../puzzles/gridlock.toml");
const ODD_SHAPES: &str = include_str!("../../puzzles/odd_shapes.toml");
const TRADING_PLACES: &str = include_str!("../../puzzles/trading_places.toml");

fn puzzle(source: &str) -> Puzzle {
    file::parse(source).unwrap().build().unwrap().0
}

fn from_grid(grid: &str, goal: (char, u8, u8)) -> Result<(Puzzle, Vec<Warning>), SpecError> {
    let mut spec = PuzzleSpec::from_grid(grid)?;
    spec.goals.push(dd_engine::GoalSpec { block: goal.0, x: goal.1, y: goal.2 });
    spec.build()
}

fn block(game: &Game, label: char) -> usize {
    game.puzzle().blocks.iter().position(|b| b.label == label).unwrap()
}

/// Deterministic pseudo-random walk through a puzzle's positions.
fn walk(puzzle: &Puzzle, steps: usize, mut visit: impl FnMut(&dd_engine::Position)) {
    let layout = &puzzle.layout;
    let mut position = puzzle.start;
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..steps {
        visit(&position);
        let mut next = Vec::new();
        layout.for_each_successor(&position, Metric::Drag, |_, p| next.push(*p));
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        position = next[seed as usize % next.len()];
    }
}

#[test]
fn klotski_classes_and_symmetry() {
    let p = puzzle(KLOTSKI);
    // Tall, big square, wide, small.
    assert_eq!(p.layout.classes().len(), 4);
    assert_eq!(p.layout.n_blocks(), 10);
    let transforms: Vec<_> = p.layout.symmetries().iter().map(|s| s.transform).collect();
    assert_eq!(transforms, [Transform::FlipH]);
}

#[test]
fn off_centre_goal_breaks_symmetry() {
    let source = KLOTSKI.replace("at = [1, 3]", "at = [0, 3]");
    assert!(puzzle(&source).layout.symmetries().is_empty());
}

#[test]
fn square_board_with_free_goal_keeps_all_fitting_symmetries() {
    // A lone 1x1 heading for the centre of a 3x3 board: all seven
    // non-identity symmetries of the square apply.
    let (p, _) = from_grid("A..\n...\n...", ('A', 1, 1)).unwrap();
    assert_eq!(p.layout.symmetries().len(), 7);
}

#[test]
fn position_codes_round_trip() {
    for source in [KLOTSKI, GRIDLOCK, ODD_SHAPES, TRADING_PLACES] {
        let p = puzzle(source);
        let layout = &p.layout;
        walk(&p, 300, |position| {
            let packed = layout.encode(position);
            assert_eq!(packed.len(), layout.code_len());
            assert_eq!(layout.decode(&packed).as_ref(), Ok(position));
            let text = layout.position_string(position);
            assert_eq!(layout.parse_position_string(&text).as_ref(), Ok(position));
            layout.check_position(position).unwrap();
        });
    }
}

#[test]
fn klotski_code_fits_in_five_bytes() {
    // 12 symbols of 3 bits.
    assert_eq!(puzzle(KLOTSKI).layout.code_len(), 5);
}

#[test]
fn mirror_images_share_a_canonical_code() {
    let p = puzzle(KLOTSKI);
    let layout = &p.layout;
    let flip = &layout.symmetries()[0];
    let (mut a, mut b, mut scratch) = (Vec::new(), Vec::new(), Vec::new());
    let mut differing = 0;
    walk(&p, 200, |position| {
        let mirrored = layout.transform(position, flip);
        layout.check_position(&mirrored).unwrap();
        assert_eq!(layout.transform(&mirrored, flip), *position);
        layout.encode_canonical_into(position, &mut a, &mut scratch);
        layout.encode_canonical_into(&mirrored, &mut b, &mut scratch);
        assert_eq!(a, b);
        differing += (mirrored != *position) as usize;
    });
    assert!(differing > 0);
}

#[test]
fn bad_position_codes_are_rejected() {
    let p = puzzle(KLOTSKI);
    assert!(p.layout.decode(&[0xff; 5]).is_err());
    assert!(p.layout.decode(&[0; 4]).is_err());
    assert!(p.layout.parse_position_string("abaacaddd..").is_err());
    assert!(p.layout.parse_position_string("bbbbbbbbbbbb").is_err());
    assert!(p.layout.parse_position_string("abaacaddd.!d").is_err());
}

#[test]
fn files_and_share_codes_round_trip() {
    for source in [KLOTSKI, GRIDLOCK, ODD_SHAPES, TRADING_PLACES] {
        let spec = file::parse(source).unwrap();
        assert_eq!(file::parse(&file::write(&spec)).unwrap(), spec);

        let decoded = PuzzleSpec::from_code(&spec.to_code().unwrap()).unwrap();
        let (original, _) = spec.build().unwrap();
        let (copy, _) = decoded.build().unwrap();
        assert_eq!(copy.start, original.start);
        assert_eq!(copy.blocks, original.blocks);
        assert_eq!(copy.layout.goal(), original.layout.goal());
        assert_eq!(decoded.cells, spec.cells);
    }
    assert_eq!(PuzzleSpec::from_code("dd1-!!"), Err(SpecError::BadCode));
    assert_eq!(PuzzleSpec::from_code("nope"), Err(SpecError::BadCode));
}

#[test]
fn file_errors_are_reported() {
    assert!(file::parse("grid = 3").is_err());
    assert!(file::parse("grid = \"AB\"\nextra = 1").is_err());
    let no_goal = file::parse("grid = \"A.\"").unwrap();
    assert_eq!(no_goal.build().unwrap_err(), SpecError::NoGoal);
    assert!(matches!(
        file::parse("grid = \"A.\"\n[goal]\nblock = \"AB\"\nat = [0, 0]"),
        Err(file::FileError::BadLabel(_))
    ));
}

#[test]
fn structural_validation() {
    let err = |grid, goal| from_grid(grid, goal).unwrap_err();
    assert_eq!(err("", ('A', 0, 0)), SpecError::EmptyGrid);
    assert_eq!(err("AB\nA", ('A', 0, 0)), SpecError::RaggedRow(1));
    assert_eq!(
        err("A?", ('A', 0, 0)),
        SpecError::BadCharacter { ch: '?', row: 0, column: 1 }
    );
    assert_eq!(err("A.A", ('A', 0, 0)), SpecError::Disconnected('A'));
    // Diagonal contact is not connection.
    assert_eq!(err("A.\n.A", ('A', 0, 0)), SpecError::Disconnected('A'));
    assert_eq!(err("A.", ('Z', 0, 0)), SpecError::UnknownBlock('Z'));
    assert_eq!(err("AA.", ('A', 2, 0)), SpecError::GoalOutOfBounds('A'));
    assert_eq!(err("A.#", ('A', 2, 0)), SpecError::GoalOutOfBounds('A'));
    let big = ".".repeat(9) + "\n";
    assert_eq!(err(&big.repeat(8), ('A', 0, 0)), SpecError::TooLarge { cells: 72 });

    let mut overlap = PuzzleSpec::from_grid("AA..\nBB..").unwrap();
    for (block, x, y) in [('A', 2, 0), ('B', 1, 0)] {
        overlap.goals.push(dd_engine::GoalSpec { block, x, y });
    }
    assert_eq!(overlap.build().unwrap_err(), SpecError::GoalOverlap('A', 'B'));
}

#[test]
fn dead_cells_are_a_warning_with_a_fix() {
    let grid = "A.#.\n..#.";
    let (p, warnings) = from_grid(grid, ('A', 1, 1)).unwrap();
    let dead = bit(p.layout.cell(3, 0)) | bit(p.layout.cell(3, 1));
    assert_eq!(warnings, [Warning::DeadCells(dead)]);

    let mut spec = PuzzleSpec::from_grid(grid).unwrap();
    spec.goals.push(dd_engine::GoalSpec { block: 'A', x: 1, y: 1 });
    let tidy = spec.with_walls(dead);
    assert_eq!(tidy.cells[3], CellSpec::Wall);
    assert!(tidy.build().unwrap().1.is_empty());
}

#[test]
fn already_solved_is_a_warning() {
    let (_, warnings) = from_grid("A.", ('A', 0, 0)).unwrap();
    assert_eq!(warnings, [Warning::AlreadySolved]);
}

#[test]
fn goal_block_is_kept_apart_from_look_alikes() {
    // Three identical 1x1 blocks; only A is the key.
    let (p, _) = from_grid("AB\nC.", ('A', 1, 1)).unwrap();
    assert_eq!(p.layout.classes().len(), 2);
    let mut game = Game::new(p);
    let c = block(&game, 'C');
    game.step(c, Dir::Right).unwrap();
    assert!(!game.is_solved(), "a look-alike on the target must not count");
}

#[test]
fn tagged_blocks_share_goals() {
    let p = puzzle(TRADING_PLACES);
    assert_eq!(p.layout.classes().len(), 2);
    assert_eq!(p.layout.goal().targets().len(), 4);
}

#[test]
fn goal_position_uses_the_bounding_box_of_irregular_shapes() {
    // S's first cell in reading order is not at the left of its bounding box.
    let (p, _) = from_grid(".S..\nSS..", ('S', 2, 0)).unwrap();
    let layout = &p.layout;
    let (_, anchor) = layout.goal().targets()[0];
    assert_eq!(layout.xy(anchor), (3, 0));
    let mut game = Game::new(p);
    let s = block(&game, 'S');
    game.step(s, Dir::Right).unwrap();
    assert!(!game.is_solved());
    game.step(s, Dir::Right).unwrap();
    assert!(game.is_solved());
    assert_eq!(game.step(s, Dir::Right), Err(dd_engine::MoveError::Blocked));
    assert_eq!(game.step(s, Dir::Up), Err(dd_engine::MoveError::Blocked));
}

#[test]
fn irregular_blocks_interlock() {
    let p = puzzle(ODD_SHAPES);
    let game = Game::new(p);
    // C sits in A's notch and cannot move at the start.
    assert_eq!(game.legal_steps(block(&game, 'C')), [false; 4]);
    assert_eq!(game.reachable(block(&game, 'C')), 0);
}

#[test]
fn axis_locks_are_enforced() {
    let mut game = Game::new(puzzle(GRIDLOCK));
    let (x, b) = (block(&game, 'X'), block(&game, 'B'));
    assert_eq!(game.puzzle().layout.class(game.puzzle().blocks[x].class).axis, Axis::Horizontal);
    assert_eq!(game.step(x, Dir::Up), Err(dd_engine::MoveError::Blocked));
    game.step(x, Dir::Right).unwrap();
    assert_eq!(game.step(b, Dir::Right), Err(dd_engine::MoveError::Blocked));
    game.step(b, Dir::Down).unwrap();
}

#[test]
fn drags_cannot_pass_through_blocks() {
    let (p, _) = from_grid("A.B.", ('A', 3, 0)).unwrap();
    let game = Game::new(p);
    let a = block(&game, 'A');
    assert_eq!(game.reachable(a), bit(1));
}

#[test]
fn drag_path_goes_round_corners() {
    let (p, _) = from_grid("A#.\n...", ('A', 2, 0)).unwrap();
    let path = p.layout.drag_path(&p.start, 0, 2).unwrap();
    assert_eq!(path, [Dir::Down, Dir::Right, Dir::Right, Dir::Up]);
    assert_eq!(p.layout.drag_path(&p.start, 0, 1), None);
}

#[test]
fn consecutive_steps_of_one_block_are_one_move() {
    let (p, _) = from_grid("A..\nB..", ('A', 2, 1)).unwrap();
    let mut game = Game::new(p);
    let (a, b) = (block(&game, 'A'), block(&game, 'B'));

    game.step(a, Dir::Right).unwrap();
    game.step(a, Dir::Right).unwrap();
    assert_eq!(game.move_count(), 1);

    // Returning to where the move began erases it.
    game.step(a, Dir::Left).unwrap();
    game.step(a, Dir::Left).unwrap();
    assert_eq!(game.move_count(), 0);
    assert_eq!(*game.position(), game.puzzle().start);

    // A, then B out and back, then A again: still one move.
    game.step(a, Dir::Right).unwrap();
    game.step(b, Dir::Right).unwrap();
    assert_eq!(game.move_count(), 2);
    game.step(b, Dir::Left).unwrap();
    assert_eq!(game.move_count(), 1);
    game.step(a, Dir::Right).unwrap();
    assert_eq!(game.move_count(), 1);
    assert_eq!(game.history()[0].from, 0);
    assert_eq!(game.history()[0].to, 2);
}

#[test]
fn undo_and_redo_work_on_whole_moves() {
    let (p, _) = from_grid("A..\nB..", ('A', 2, 1)).unwrap();
    let mut game = Game::new(p);
    let (a, b) = (block(&game, 'A'), block(&game, 'B'));
    game.step(a, Dir::Right).unwrap();
    game.step(a, Dir::Right).unwrap();
    game.step(b, Dir::Right).unwrap();
    let after = *game.position();

    assert_eq!(game.undo().unwrap().block, b);
    assert_eq!(game.undo().unwrap().block, a);
    assert!(game.undo().is_none());
    assert_eq!(*game.position(), game.puzzle().start);

    game.redo().unwrap();
    assert_eq!(game.anchor(a), Some(2));
    game.redo().unwrap();
    assert!(game.redo().is_none());
    assert_eq!(*game.position(), after);
    assert_eq!(game.move_count(), 2);

    // A new move discards what was undone.
    game.undo().unwrap();
    game.step(a, Dir::Down).unwrap();
    assert!(game.redo().is_none());

    game.reset();
    assert_eq!(game.move_count(), 0);
    assert_eq!(*game.position(), game.puzzle().start);
}

#[test]
fn solver_moves_apply_to_the_right_block() {
    let (p, _) = from_grid("AB.", ('A', 2, 0)).unwrap();
    let class = p.blocks[1].class;
    let mut game = Game::new(p);
    assert_eq!(game.apply(Move { class, from: 1, to: 2 }), Ok(1));
    assert!(game.apply(Move { class, from: 1, to: 2 }).is_err());
}

#[test]
fn loading_a_position_keeps_block_counts() {
    let p = puzzle(KLOTSKI);
    let mut last = p.start;
    walk(&p, 50, |position| last = *position);
    let mut game = Game::new(p);
    game.load_position(&last).unwrap();
    assert_eq!(*game.position(), last);
    let covered = (0..game.block_count()).fold(0, |acc, b| acc | game.block_cells(b));
    assert_eq!(covered, game.layout().occupancy(&last));
    assert_eq!(game.move_count(), 0);
}

#[test]
fn exits_are_inferred_given_and_checked() {
    use dd_engine::Dir;
    assert_eq!(puzzle(KLOTSKI).exit, Some(Dir::Down));
    assert_eq!(puzzle(GRIDLOCK).exit, Some(Dir::Right));
    // Top-right corner: down and left are not touched; right wins over up.
    assert_eq!(puzzle(ODD_SHAPES).exit, Some(Dir::Right));
    assert_eq!(puzzle(TRADING_PLACES).exit, None);
    let (middle, _) = from_grid("A..\n...\n...", ('A', 1, 1)).unwrap();
    assert_eq!(middle.exit, None);

    let given = ODD_SHAPES.replace("[goal]", "exit = \"up\"\n\n[goal]");
    assert_eq!(puzzle(&given).exit, Some(Dir::Up));
    let spec = file::parse(&given).unwrap();
    assert_eq!(file::parse(&file::write(&spec)).unwrap(), spec);
    assert_eq!(PuzzleSpec::from_code(&spec.to_code().unwrap()).unwrap().exit, Some(Dir::Up));

    let wrong = ODD_SHAPES.replace("[goal]", "exit = \"down\"\n\n[goal]");
    assert_eq!(file::parse(&wrong).unwrap().build().unwrap_err(), SpecError::BadExit);
    let several = TRADING_PLACES.replacen("[[goal]]", "exit = \"up\"\n\n[[goal]]", 1);
    assert_eq!(file::parse(&several).unwrap().build().unwrap_err(), SpecError::BadExit);
}
