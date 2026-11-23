//! `dd`: sliding-block puzzles in the terminal.
//!
//! For now this is the headless half: load a puzzle, inspect it, solve it.

mod app;
mod data;
mod look;
mod render;
mod puzzles;
mod settings;
mod ui;

use std::fmt::Write as _;
use std::io::{IsTerminal as _, Write as _};
use std::process::ExitCode;
use std::time::Instant;

use dd_engine::{file, Game, Metric, Puzzle, PuzzleSpec, Warning};
use dd_solver::{Explore, Options, Outcome, Solution, Solver};


const USAGE: &str = "\
usage: dd [command]

  (no command)             start the game
  play <puzzle>            start the game on a puzzle
  list                     built-in puzzles
  show <puzzle>            draw a puzzle and check it
  solve <puzzle> [options] find the shortest solution
  code <puzzle>            print the puzzle's share code
  export <puzzle>          print the puzzle as a file
  preview <puzzle> [opts]  draw the puzzle in colour (needs a 24-bit terminal)

  settings [init]          show your settings and where they live, or
                           write a settings file with the defaults
  settings move-data <dir> copy puzzles, progress and records to <dir> and
                           use it from now on (nothing is deleted)

preview options (defaults come from your settings):
  --theme <name|all>  dark, light, escape-dark, escape-light, liberation,
                      or all (the default)
  --scale <n>         characters per cell across, even, 4 or more
  --grain <0-3>       texture strength; 2 is the theme's own
  --labels            write each block's label on it
  --goal              outline where the key block has to go
  --grid / --no-grid  outline empty cells, or not
  --select <label>    outline one block as if selected
  --play              animate the shortest solution, then the exit
  --exit              jump to the solved position and animate the exit

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
            for (id, source) in puzzles::BUILT_IN {
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
        Some("settings") => show_settings(&args[1..], out),
        None => interactive(None),
        Some("play") => {
            let arg = puzzle_arg()?;
            let source = if puzzles::BUILT_IN.iter().any(|(n, _)| n == arg) {
                data::library::Source::BuiltIn { name: arg.clone() }
            } else {
                data::library::Source::Code
            };
            interactive(Some((load(arg)?, source)))
        }
        Some("snapshot") => snapshot(&args[1..], out),
        Some("help" | "--help" | "-h") => {
            put!(out, "{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

fn load(arg: &str) -> Result<PuzzleSpec, String> {
    if let Some((_, source)) = puzzles::BUILT_IN.iter().find(|(id, _)| *id == arg) {
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

fn show_settings(args: &[String], out: &mut String) -> Result<(), String> {
    let home = data::Home::from_env();
    let path = home.settings_file();
    let shown = path.display();
    match args.first().map(String::as_str) {
        None => {
            let current = settings::Settings::load(&home)?;
            say!(out, "# {shown}{}", if path.exists() { "" } else { " (not created; these are the defaults)" });
            say!(out, "# puzzles, progress and records: {}", current.data_dir(&home).display());
            put!(out, "{}", current.to_toml());
            Ok(())
        }
        Some("init") => {
            if path.exists() {
                return Err(format!("{shown} already exists"));
            }
            settings::Settings::default().save(&home)?;
            say!(out, "wrote {shown}");
            Ok(())
        }
        Some("move-data") => {
            let target = args.get(1).ok_or("move-data needs a folder")?;
            let target = std::path::absolute(target).map_err(|e| format!("{target}: {e}"))?;
            let mut current = settings::Settings::load(&home)?;
            let old = current.data_dir(&home);
            let report = data::relocate::copy_data(&old, &target)?;
            current.data_dir = (target != home.data_dir(None)).then_some(target.clone());
            current.save(&home)?;
            say!(out, "copied {} file(s) to {}", report.copied, target.display());
            for skipped in &report.skipped {
                say!(out, "skipped {} (already there; left as it was)", skipped.display());
            }
            if old != target {
                say!(out, "the old copy is still in {}; remove it yourself once you're happy", old.display());
            }
            Ok(())
        }
        Some(other) => Err(format!("unknown settings action {other:?}")),
    }
}

fn preview(spec: &PuzzleSpec, flags: &[String], out: &mut String) -> Result<(), String> {
    let settings = settings::Settings::load(&data::Home::from_env())?;
    let mut themes: Vec<&look::Theme> = look::THEMES.iter().collect();
    let mut options = settings.paint_options();
    let mut labels = settings.labels == settings::Labels::Always;
    let mut select = None;
    let (mut play, mut exit) = (false, false);
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
            "--grain" => {
                let level: u8 = parse(value()?)?;
                if level > 3 {
                    return Err("--grain takes 0, 1, 2 or 3".into());
                }
                options.grain = settings::grain_strength(level);
            }
            "--labels" => labels = true,
            "--goal" => options.show_goal = true,
            "--grid" => options.grid = true,
            "--no-grid" => options.grid = false,
            "--select" => select = Some(value()?.clone()),
            "--play" => play = true,
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
    if !play && !exit {
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
    let moves = solver.solution().expect("solved").moves;
    let mut player = Player::new(out);
    let theme = themes[0];
    if exit {
        for &mv in &moves {
            game.apply(mv).map_err(|e| e.to_string())?;
        }
    } else {
        player.hold(&look::paint(&game, theme, &options), 600);
        for &mv in &moves {
            let path = game.layout().drag_path(game.position(), mv.from, mv.to).unwrap_or_default();
            let block = game.apply(mv).map_err(|e| e.to_string())?;
            let millis = look::slide_millis(path.len());
            for t in player.times(millis) {
                let offset = look::slide_offset(&path, options.scale, t);
                player.frame(&look::paint_moving(&game, theme, &options, look::Motion { block, offset, opacity: 1.0 }));
            }
        }
    }
    player.hold(&look::paint(&game, theme, &options), 300);
    if let Some(block) = look::exiting_block(&game) {
        for t in player.times(look::EXIT_MILLIS) {
            let opacity = look::exit_opacity(t);
            player.frame(&look::paint_moving(&game, theme, &options, look::Motion { block, offset: (0, 0), opacity }));
        }
    }
    Ok(())
}

/// Shows animation frames: in place on a terminal, at a fixed rate; or, when
/// output is not a terminal, as ANSI frames separated by form feeds, one per
/// tick, for other tools to turn into video.
struct Player<'a> {
    out: &'a mut String,
    live: bool,
    height: Option<usize>,
}

impl<'a> Player<'a> {
    const TICK_MILLIS: u64 = 33;

    fn new(out: &'a mut String) -> Self {
        Player { out, live: std::io::stdout().is_terminal(), height: None }
    }

    /// Frame times from just after 0 to 1 for an animation lasting `millis`.
    fn times(&self, millis: u64) -> Vec<f32> {
        let n = millis.div_ceil(Self::TICK_MILLIS).max(1);
        (1..=n).map(|i| i as f32 / n as f32).collect()
    }

    fn frame(&mut self, canvas: &look::Canvas) {
        if self.live {
            let mut stdout = std::io::stdout().lock();
            if let Some(h) = self.height {
                let _ = write!(stdout, "\x1b[{h}A");
            }
            let _ = stdout.write_all(canvas.to_ansi().as_bytes());
            let _ = stdout.flush();
            self.height = Some(canvas.height);
            std::thread::sleep(std::time::Duration::from_millis(Self::TICK_MILLIS));
        } else {
            put!(self.out, "{}\x0c", canvas.to_ansi());
        }
    }

    /// Shows a frame for about `millis`.
    fn hold(&mut self, canvas: &look::Canvas, millis: u64) {
        for _ in 0..millis.div_ceil(Self::TICK_MILLIS) {
            self.frame(canvas);
        }
    }
}

/// Starts the interactive app, optionally on a puzzle.
fn interactive(puzzle: Option<(PuzzleSpec, data::library::Source)>) -> Result<(), String> {
    if !std::io::stdout().is_terminal() {
        return Err("the game needs a terminal; `dd help` lists the other commands".into());
    }
    let mut app = app::App::new(data::Home::from_env())?;
    if let Some((spec, source)) = puzzle {
        app.open_puzzle(spec, source);
    }
    app::run::run(&mut app).map_err(|e| e.to_string())
}

/// `dd snapshot [--size WxH] [--keys a,b,c] [puzzle]`: draws the app after
/// the given keys, without a terminal. For screenshots and checks.
fn snapshot(args: &[String], out: &mut String) -> Result<(), String> {
    let (mut width, mut height) = (100u16, 32u16);
    let mut keys = Vec::new();
    let mut puzzle = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--size" => {
                let size = args.next().ok_or("--size needs WxH")?;
                let (w, h) = size.split_once('x').ok_or("--size needs WxH")?;
                (width, height) = (parse(w)?, parse(h)?);
            }
            "--keys" => {
                for name in args.next().ok_or("--keys needs a list")?.split(',') {
                    keys.push(app::run::key_named(name).ok_or(format!("unknown key {name:?}"))?);
                }
            }
            other => puzzle = Some(other.to_string()),
        }
    }
    let mut app = app::App::new(data::Home::from_env())?;
    if let Some(name) = puzzle {
        let source = if puzzles::BUILT_IN.iter().any(|(n, _)| *n == name) {
            data::library::Source::BuiltIn { name: name.clone() }
        } else {
            data::library::Source::Code
        };
        app.open_puzzle(load(&name)?, source);
    }
    for key in keys {
        if app.handle(key) == app::Flow::Quit {
            break;
        }
    }
    // Let any slide, then any exit, finish: the picture shows where things
    // settle.
    let later = Instant::now() + std::time::Duration::from_secs(60);
    app.tick(later);
    app.tick(later + std::time::Duration::from_secs(60));
    put!(out, "{}", app::run::snapshot(&app, width, height));
    Ok(())
}
