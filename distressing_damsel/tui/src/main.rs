//! `dd`: sliding-block puzzles in the terminal.
//!
//! For now this is the headless half: load a puzzle, inspect it, solve it.

mod look;
mod render;
mod settings;

use std::fmt::Write as _;
use std::io::{IsTerminal as _, Write as _};
use std::process::ExitCode;
use std::time::Instant;

use dd_engine::{file, Game, Metric, Puzzle, PuzzleSpec, Warning};
use dd_solver::{Explore, Options, Outcome, Solution, Solver};

const BUILT_IN: &[(&str, &str)] = &[
    ("first_steps", include_str!("../../puzzles/first_steps.toml")),
    ("klotski", include_str!("../../puzzles/klotski.toml")),
    ("gridlock", include_str!("../../puzzles/gridlock.toml")),
    ("odd_shapes", include_str!("../../puzzles/odd_shapes.toml")),
    ("trading_places", include_str!("../../puzzles/trading_places.toml")),
];

const USAGE: &str = "\
usage: dd <command>

  list                     built-in puzzles
  show <puzzle>            draw a puzzle and check it
  solve <puzzle> [options] find the shortest solution
  code <puzzle>            print the puzzle's share code
  export <puzzle>          print the puzzle as a file
  preview <puzzle> [opts]  draw the puzzle in colour (needs a 24-bit terminal)

  settings [init]          show your settings and where they live, or
                           write a settings file with the defaults

preview options (defaults come from your settings):
  --theme <name|all>  dark, light, escape-dark, escape-light, heirloom, or
                      all (the default)
  --scale <n>         characters per cell across, even, 4 or more
  --grain <x>         texture strength, 0 to 2
  --labels            write each block's label on it
  --goal              outline where the key block has to go
  --select <label>    outline one block as if selected
  --exit              solve the puzzle and play the key block's exit

<puzzle> is a built-in name, a path to a .toml file, or a share code.

solve options:
  --boards            draw the board after every move
  --alternatives <n>  show up to n different shortest solutions
  --everything        keep searching after solving, to map all positions
  --factor <x>        with --everything, stop at x times the positions held
                      when solved (default 2)
  --max-states <n>    give up after storing n positions (default 5000000)
  --steps             count single-cell steps instead of drags
  --no-symmetry       do not merge mirror-image positions
";

// Output is gathered and written once, so that a closed pipe (`dd ... | head`)
// ends the program quietly instead of panicking mid-print.
macro_rules! say {
    ($out:expr, $($arg:tt)*) => {{ let _ = writeln!($out, $($arg)*); }};
}
macro_rules! put {
    ($out:expr, $($arg:tt)*) => {{ let _ = write!($out, $($arg)*); }};
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = String::new();
    let result = run(&args, &mut out);
    let _ = std::io::stdout().write_all(out.as_bytes());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("dd: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String], out: &mut String) -> Result<(), String> {
    let puzzle_arg = || args.get(1).ok_or_else(|| "which puzzle?".to_string());
    match args.first().map(String::as_str) {
        Some("list") => {
            for (id, source) in BUILT_IN {
                let spec = file::parse(source).map_err(|e| format!("{id}: {e}"))?;
                say!(out, "{id:<16} {}", spec.description);
            }
            Ok(())
        }
        Some("show") => show(&load(puzzle_arg()?)?, out),
        Some("solve") => solve(&load(puzzle_arg()?)?, &args[2..], out),
        Some("code") => {
            say!(out, "{}", load(puzzle_arg()?)?.to_code().map_err(|e| e.to_string())?);
            Ok(())
        }
        Some("export") => {
            put!(out, "{}", file::write(&load(puzzle_arg()?)?));
            Ok(())
        }
        Some("preview") => preview(&load(puzzle_arg()?)?, &args[2..], out),
        Some("settings") => show_settings(args.get(1).map(String::as_str), out),
        Some("help" | "--help" | "-h") | None => {
            put!(out, "{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

fn load(arg: &str) -> Result<PuzzleSpec, String> {
    if let Some((_, source)) = BUILT_IN.iter().find(|(id, _)| *id == arg) {
        return file::parse(source).map_err(|e| e.to_string());
    }
    if arg.starts_with("dd1-") {
        return PuzzleSpec::from_code(arg).map_err(|e| e.to_string());
    }
    let source = std::fs::read_to_string(arg).map_err(|e| format!("{arg}: {e}"))?;
    file::parse(&source).map_err(|e| format!("{arg}: {e}"))
}

fn build(spec: &PuzzleSpec, out: &mut String) -> Result<Puzzle, String> {
    let (puzzle, warnings) = spec.build().map_err(|e| e.to_string())?;
    for warning in warnings {
        match warning {
            Warning::DeadCells(mask) => say!(out, 
                "note: {} open cell(s) can never be entered and could be walls",
                mask.count_ones()
            ),
            Warning::AlreadySolved => say!(out, "note: the starting position already meets the goal"),
        }
    }
    Ok(puzzle)
}

fn show(spec: &PuzzleSpec, out: &mut String) -> Result<(), String> {
    let puzzle = build(spec, out)?;
    let layout = puzzle.layout.clone();
    if !puzzle.name.is_empty() {
        say!(out, "{}", puzzle.name);
    }
    if !puzzle.description.is_empty() {
        say!(out, "{}", puzzle.description);
    }
    say!(out, "");
    let game = Game::new(puzzle);
    put!(out, "{}", render::board(&game));
    say!(out, "");
    say!(out, 
        "{} by {}, {} blocks in {} classes, {} symmetries",
        layout.width(),
        layout.height(),
        layout.n_blocks(),
        layout.classes().len(),
        layout.symmetries().len()
    );
    say!(out, "position {}", layout.position_string(game.position()));
    say!(out, "code     {}", spec.to_code().map_err(|e| e.to_string())?);
    Ok(())
}

fn solve(spec: &PuzzleSpec, flags: &[String], out: &mut String) -> Result<(), String> {
    let mut options = Options::default();
    let mut boards = false;
    let mut alternatives = 1usize;
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        let mut value = |what: &str| flags.next().ok_or(format!("{flag} needs {what}"));
        match flag.as_str() {
            "--boards" => boards = true,
            "--everything" => options.explore = Explore::Everything,
            "--steps" => options.metric = Metric::Step,
            "--no-symmetry" => options.symmetry = false,
            "--alternatives" => alternatives = parse(value("a number")?)?,
            "--max-states" => options.max_states = parse(value("a number")?)?,
            "--factor" => options.post_solve_factor = parse(value("a number")?)?,
            other => return Err(format!("unknown option {other:?}")),
        }
    }

    let puzzle = build(spec, out)?;
    let started = Instant::now();
    let mut solver = Solver::new(puzzle.layout.clone(), puzzle.start, options);
    let outcome = solver.run();
    let progress = solver.progress();
    let elapsed = started.elapsed();

    match outcome {
        Outcome::Unsolvable => {
            say!(out, "no solution: all {} reachable positions checked", progress.states);
            return Ok(());
        }
        Outcome::GaveUp => {
            say!(out, 
                "gave up after {} positions (depth {}) without a solution",
                progress.states, progress.depth
            );
            return Ok(());
        }
        Outcome::Solved => {}
    }

    let unit = if options.metric == Metric::Step { "steps" } else { "moves" };
    let length = progress.solution_len.unwrap_or(0);
    say!(out, "solved in {length} {unit}");
    say!(out, "  positions stored   {} in {:.1?}", progress.states, elapsed);
    if let Some(count) = solver.shortest_count() {
        let bound = if solver.shortest_is_complete() { "" } else { "at least " };
        say!(out, "  shortest solutions {bound}{count}");
    }
    let histogram = solver.depth_histogram();
    say!(out, "  positions by depth {}", render::sparkline(&histogram));
    if options.explore == Explore::Everything {
        let map = solver.distance_map();
        let far = map.histogram().len().saturating_sub(1);
        let scope = if map.is_exact() { "all positions mapped" } else { "partial map" };
        say!(out, "  farthest from goal {far} {unit} ({scope})");
    }

    let solutions = if alternatives > 1 {
        solver.solutions(alternatives)
    } else {
        solver.solution().into_iter().collect()
    };
    for (i, solution) in solutions.iter().enumerate() {
        say!(out, "");
        if solutions.len() > 1 {
            say!(out, "solution {} of {}", i + 1, solutions.len());
        }
        print_solution(&puzzle, solution, boards, out)?;
    }
    Ok(())
}

fn print_solution(puzzle: &Puzzle, solution: &Solution, boards: bool, out: &mut String) -> Result<(), String> {
    let mut game = Game::new(puzzle.clone());
    if boards {
        put!(out, "{}", render::board(&game));
    }
    let mut line = String::new();
    for (i, &mv) in solution.moves.iter().enumerate() {
        let path = game
            .layout()
            .drag_path(game.position(), mv.from, mv.to)
            .unwrap_or_default();
        let block = game.apply(mv).map_err(|e| format!("solution does not replay: {e}"))?;
        let arrows: String = path.iter().map(|d| d.arrow()).collect();
        let text = format!("{}{arrows}", puzzle.blocks[block].label);
        if boards {
            say!(out, "\n{:>3}. {text}", i + 1);
            put!(out, "{}", render::board(&game));
        } else {
            if line.chars().count() + text.chars().count() > 70 {
                say!(out, "  {}", line.trim_end());
                line.clear();
            }
            line.push_str(&text);
            line.push_str("  ");
        }
    }
    if !line.is_empty() {
        say!(out, "  {}", line.trim_end());
    }
    if !game.is_solved() {
        return Err("solution does not reach the goal".into());
    }
    Ok(())
}

fn parse<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse().map_err(|_| format!("{text:?} is not a number"))
}

fn show_settings(action: Option<&str>, out: &mut String) -> Result<(), String> {
    let path = settings::Settings::path();
    let shown = path.as_ref().map_or("(no home directory)".into(), |p| p.display().to_string());
    match action {
        None => {
            let current = settings::Settings::load()?;
            let exists = path.as_ref().is_some_and(|p| p.exists());
            say!(out, "# {shown}{}", if exists { "" } else { " (not created; these are the defaults)" });
            put!(out, "{}", current.to_toml());
            Ok(())
        }
        Some("init") => {
            let path = path.ok_or("no home directory to put settings in")?;
            if path.exists() {
                return Err(format!("{shown} already exists"));
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            std::fs::write(&path, settings::Settings::default().to_toml()).map_err(|e| format!("{shown}: {e}"))?;
            say!(out, "wrote {shown}");
            Ok(())
        }
        Some(other) => Err(format!("unknown settings action {other:?}")),
    }
}

fn preview(spec: &PuzzleSpec, flags: &[String], out: &mut String) -> Result<(), String> {
    let settings = settings::Settings::load()?;
    let mut themes: Vec<&look::Theme> = look::THEMES.iter().collect();
    let mut options = settings.paint_options();
    let mut labels = settings.labels == settings::Labels::Always;
    let mut select = None;
    let mut exit = false;
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        let mut value = || flags.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--theme" => {
                let name = value()?;
                if name != "all" {
                    themes = vec![look::theme::by_name(name).ok_or(format!("no theme called {name:?}"))?];
                }
            }
            "--scale" => options.scale = parse(value()?)?,
            "--grain" => options.grain = parse(value()?)?,
            "--labels" => labels = true,
            "--goal" => options.show_goal = true,
            "--select" => select = Some(value()?.clone()),
            "--exit" => exit = true,
            other => return Err(format!("unknown option {other:?}")),
        }
    }
    let puzzle = build(spec, out)?;
    let mut game = Game::new(puzzle);
    let blocks = &game.puzzle().blocks;
    if labels {
        options.labels = blocks.iter().enumerate().map(|(i, b)| (i, b.label.to_string())).collect();
    }
    if let Some(label) = select {
        let id = blocks.iter().position(|b| b.label.to_string() == label);
        options.selected = Some(id.ok_or(format!("no block {label}"))?);
    }
    if !exit {
        for theme in themes {
            say!(out, "{}", theme.name);
            put!(out, "{}", look::paint(&game, theme, &options).to_ansi());
            say!(out, "");
        }
        return Ok(());
    }

    let mut solver = Solver::new(game.puzzle().layout.clone(), game.puzzle().start, Options::default());
    if solver.run() != Outcome::Solved {
        return Err("the puzzle has no solution to play out".into());
    }
    for mv in solver.solution().expect("solved").moves {
        game.apply(mv).map_err(|e| e.to_string())?;
    }
    let theme = themes[0];
    let frames = look::exit_frames(&game, theme, &options);
    if std::io::stdout().is_terminal() {
        // Play it: draw each frame over the last.
        let mut stdout = std::io::stdout().lock();
        let height = frames[0].height;
        for (i, frame) in frames.iter().enumerate() {
            if i > 0 {
                let _ = write!(stdout, "\x1b[{height}A");
            }
            let _ = stdout.write_all(frame.to_ansi().as_bytes());
            let _ = stdout.flush();
            std::thread::sleep(std::time::Duration::from_millis(if i == 0 { 500 } else { 45 }));
        }
    } else {
        // Not a terminal: frames separated by form feeds, for other tools.
        for frame in frames {
            put!(out, "{}\x0c", frame.to_ansi());
        }
    }
    Ok(())
}
