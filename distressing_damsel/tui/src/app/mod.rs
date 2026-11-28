//! The interactive app: one screen, a stack of overlays on top, and keys
//! dispatched through the tables in [`keys`].
//!
//! `App::handle` takes a key and updates the state; `App::draw` paints it.
//! Neither touches the terminal, so both are tested directly. Loading and
//! saving go through [`crate::data`] in the app's [`Home`] folder.

pub mod cursor;
pub mod draw;
pub mod game;
pub mod hints;
pub mod keys;
pub mod menu;
pub mod play;
pub mod prefs;
pub mod replay;
pub mod run;
pub mod solving;

use std::time::Instant;

use dd_engine::{file, PuzzleSpec};

use crate::data::library::{self, LastPuzzle, Source};
use crate::data::records::{Records, Score};
use crate::data::Home;
use crate::puzzles::BUILT_IN;
use crate::settings::Settings;
use game::GameScreen;
use keys::{Action, Context, Key};
use menu::{item, not_yet, Menu};

/// Whether the app keeps running after a key.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HomeItem {
    Continue,
    Open,
    New,
    Settings,
    Quit,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PauseItem {
    Resume,
    Restart,
    Checkpoints,
    Solution,
    Picture,
    Open,
    New,
    Quit,
}

/// A puzzle offered on the Open screen.
#[derive(Clone, Debug)]
pub enum Entry {
    BuiltIn { name: &'static str, title: String },
    File { file: String, title: String },
    Broken { file: String, error: String },
}

pub enum Screen {
    Home(Menu<HomeItem>),
    Open(Menu<usize>, Vec<Entry>),
    Game(Box<GameScreen>),
    /// A screen that is not built yet.
    Placeholder { title: &'static str, note: &'static str },
}

pub enum Overlay {
    /// Keys for the context underneath.
    Help(Context),
    Pause(Menu<PauseItem>),
    Confirm(Confirm),
    /// The preferences list, and which row is chosen.
    Preferences(usize),
    /// After a solve.
    Summary(Summary),
    /// About to save a picture: PNG (true) or ANSI.
    Picture { png: bool },
}

/// What the summary after a solve shows. The shortest length is read from
/// the solver when drawn, as it may still be working.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub score: Score,
    pub new_best: bool,
}

/// What a yes-or-no dialogue is asking about.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Question {
    Hint,
    Restart,
    /// Before restarting: keep the current position as a checkpoint?
    KeepBeforeRestart,
}

/// A yes-or-no dialogue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub question: Question,
    pub title: &'static str,
    pub text: &'static str,
    /// Offered only where answering once may be enough: whether it is
    /// ticked.
    pub dont_ask: Option<bool>,
    /// Which button Enter presses.
    pub yes_focused: bool,
}

impl Confirm {
    fn hint() -> Confirm {
        Confirm {
            question: Question::Hint,
            title: " Hint ",
            text: "Take a hint? It counts towards this attempt.",
            dont_ask: Some(false),
            yes_focused: true,
        }
    }

    fn restart() -> Confirm {
        Confirm {
            question: Question::Restart,
            title: " Restart ",
            text: "Restart this puzzle? Your moves, hints and solution views start again.",
            dont_ask: None,
            yes_focused: false,
        }
    }

    fn keep_before_restart() -> Confirm {
        Confirm {
            question: Question::KeepBeforeRestart,
            title: " Keep this position? ",
            text: "Keep your current position as a checkpoint before restarting?",
            dont_ask: None,
            yes_focused: true,
        }
    }
}

pub struct App {
    pub home: Home,
    pub settings: Settings,
    pub records: Records,
    pub screen: Screen,
    pub overlays: Vec<Overlay>,
    /// Shown in the footer until the next key.
    pub message: Option<String>,
    /// The summary of the last solve: waiting for the exit to finish, and
    /// kept to show again after a replay.
    pub summary: Option<Summary>,
    summary_waiting: bool,
    /// The size of the last frame drawn, for ANSI pictures of the screen.
    pub size: std::cell::Cell<(u16, u16)>,
}

impl App {
    pub fn new(home: Home) -> Result<App, String> {
        let settings = Settings::load(&home)?;
        let data_dir = settings.data_dir(&home);
        let (records, message) = match Records::load(&data_dir) {
            Ok(records) => (records, None),
            Err(e) => (Records::default(), Some(format!("could not read your records: {e}"))),
        };
        let mut app = App {
            home,
            settings,
            records,
            screen: Screen::Placeholder { title: "", note: "" },
            overlays: Vec::new(),
            message,
            summary: None,
            summary_waiting: false,
            size: std::cell::Cell::new((100, 32)),
        };
        app.go_home();
        Ok(app)
    }

    fn data_dir(&self) -> std::path::PathBuf {
        self.settings.data_dir(&self.home)
    }

    /// Whether something is moving and the screen needs redrawing without a
    /// key press.
    pub fn animating(&self) -> bool {
        let now = Instant::now();
        matches!(&self.screen, Screen::Game(g) if g.animating() || g.pending_exit.is_some() || g.busy(now))
    }

    /// Whether a background search is still running.
    pub fn searching(&self) -> bool {
        matches!(&self.screen, Screen::Game(g)
            if matches!(g.solver, hints::SolverState::Running(_)) || g.hint_search.is_some())
    }

    /// Moves animations on to `now`, collects finished searches, and shows
    /// the summary once a solve's exit or a replay has finished.
    pub fn tick(&mut self, now: Instant) {
        let Screen::Game(game) = &mut self.screen else { return };
        game.advance(now);
        game.poll_solver(now);
        if game.replay.as_ref().is_some_and(|r| r.finished) {
            game.replay = None;
            self.summary_waiting = true;
        }
        let settled = !game.animating() && game.pending_exit.is_none() && game.replay.is_none();
        if self.summary_waiting && settled && self.overlays.is_empty() {
            if let Some(summary) = self.summary {
                self.overlays.push(Overlay::Summary(summary));
            }
            self.summary_waiting = false;
        }
    }

    /// The context whose keys are live.
    pub fn context(&self) -> Context {
        match self.overlays.last() {
            Some(Overlay::Help(_)) => Context::Help,
            Some(Overlay::Pause(_)) => Context::Pause,
            Some(Overlay::Confirm(_)) => Context::Confirm,
            Some(Overlay::Preferences(_)) => Context::Preferences,
            Some(Overlay::Summary(_)) => Context::Summary,
            Some(Overlay::Picture { .. }) => Context::Picture,
            None => match &self.screen {
                Screen::Home(_) => Context::Home,
                Screen::Open(..) => Context::List,
                Screen::Game(game) => game.context(),
                Screen::Placeholder { .. } => Context::Placeholder,
            },
        }
    }

    pub fn handle(&mut self, key: Key) -> Flow {
        self.message = None;
        let Some(action) = keys::lookup(self.context(), key) else {
            return Flow::Continue;
        };
        if action == Action::Quit {
            self.leave_game();
            return Flow::Quit;
        }
        if let Some(overlay) = self.overlays.last_mut() {
            match overlay {
                Overlay::Help(_) => {
                    if action == Action::Back {
                        self.overlays.pop();
                    }
                }
                Overlay::Picture { png } => match action {
                    Action::SwitchFormat => *png = !*png,
                    Action::ToggleLabels => {
                        if let Screen::Game(game) = &mut self.screen {
                            game.picture = game.picture.map(|on| !on);
                        }
                    }
                    Action::Save => {
                        let png = *png;
                        self.save_picture(png);
                    }
                    Action::Back => {
                        self.overlays.pop();
                        if let Screen::Game(game) = &mut self.screen {
                            game.picture = None;
                        }
                    }
                    _ => {}
                },
                Overlay::Summary(_) => match action {
                    Action::Back => {
                        self.overlays.pop();
                    }
                    Action::Replay => {
                        self.overlays.pop();
                        if let Screen::Game(game) = &mut self.screen {
                            game.start_replay(Instant::now(), self.settings.animations);
                        }
                    }
                    Action::OpenAnother => {
                        self.leave_game();
                        self.go_open();
                    }
                    _ => {}
                },
                Overlay::Preferences(row) => {
                    let rows = prefs::rows(&self.settings);
                    *row = (*row).min(rows.len() - 1);
                    match action {
                        Action::Up => *row = (*row + rows.len() - 1) % rows.len(),
                        Action::Down => *row = (*row + 1) % rows.len(),
                        Action::Adjust(forward) => {
                            prefs::adjust(&mut self.settings, rows[*row], forward);
                            if let Err(e) = self.settings.save(&self.home) {
                                self.message = Some(e);
                            }
                        }
                        Action::Back => {
                            self.overlays.pop();
                        }
                        _ => {}
                    }
                }
                Overlay::Confirm(confirm) => {
                    let answer = match action {
                        Action::Yes => Some(true),
                        Action::No => Some(false),
                        Action::Choose => Some(confirm.yes_focused),
                        Action::NextChoice => {
                            confirm.yes_focused = !confirm.yes_focused;
                            None
                        }
                        Action::ToggleDontAsk => {
                            if let Some(ticked) = &mut confirm.dont_ask {
                                *ticked = !*ticked;
                            }
                            None
                        }
                        _ => None,
                    };
                    if let Some(yes) = answer {
                        let confirm = confirm.clone();
                        self.overlays.pop();
                        self.answer(&confirm, yes);
                    }
                }
                Overlay::Pause(menu) => {
                    let chosen = match action {
                        Action::Resume => Some(PauseItem::Resume),
                        Action::Up => {
                            menu.step(-1);
                            None
                        }
                        Action::Down => {
                            menu.step(1);
                            None
                        }
                        Action::Choose => menu.chosen(),
                        Action::Shortcut(c) => menu.by_letter(c),
                        _ => None,
                    };
                    if let Some(item) = chosen {
                        return self.pause_choice(item);
                    }
                }
            }
            return Flow::Continue;
        }
        match &mut self.screen {
            Screen::Home(menu) => {
                let chosen = match action {
                    Action::Up => {
                        menu.step(-1);
                        None
                    }
                    Action::Down => {
                        menu.step(1);
                        None
                    }
                    Action::Choose => menu.chosen(),
                    Action::Shortcut(c) => menu.by_letter(c),
                    _ => None,
                };
                if let Some(item) = chosen {
                    return self.home_choice(item);
                }
            }
            Screen::Open(menu, entries) => match action {
                Action::Up => menu.step(-1),
                Action::Down => menu.step(1),
                Action::Back => self.go_home(),
                Action::Choose => {
                    if let Some(entry) = menu.chosen().and_then(|i| entries.get(i)).cloned() {
                        self.open_entry(&entry);
                    }
                }
                _ => {}
            },
            Screen::Game(game) if game.replay.is_some() => {
                if action == Action::Back {
                    game.replay = None;
                    self.summary_waiting = true;
                }
            }
            Screen::Game(game) => match action {
                Action::Help => {
                    let context = game.context();
                    self.overlays.push(Overlay::Help(context));
                }
                Action::Pause => {
                    game.snap();
                    self.overlays.push(Overlay::Pause(pause_menu()));
                }
                Action::Hint => match game.hint_ready() {
                    Err(why) => self.message = Some(why.to_string()),
                    Ok(()) if self.settings.confirm_hints => self.overlays.push(Overlay::Confirm(Confirm::hint())),
                    Ok(()) => game.give_hint(Instant::now()),
                },
                Action::Preferences => self.overlays.push(Overlay::Preferences(0)),
                Action::ToggleGoal => {
                    self.settings.show_goal = !self.settings.show_goal;
                    if let Err(e) = self.settings.save(&self.home) {
                        self.message = Some(e);
                    }
                }
                action => {
                    if game.act(action, Instant::now(), self.settings.animations) == Some(play::Event::Solved) {
                        self.record_solve();
                    }
                }
            },
            Screen::Placeholder { .. } => {
                if action == Action::Back {
                    self.go_home();
                }
            }
        }
        Flow::Continue
    }

    fn home_choice(&mut self, item: HomeItem) -> Flow {
        match item {
            HomeItem::Continue => self.continue_last(),
            HomeItem::Open => self.go_open(),
            HomeItem::New => self.screen = new_puzzle_placeholder(),
            HomeItem::Settings => {
                self.screen = Screen::Placeholder {
                    title: "Settings",
                    note: "The settings screen arrives in a later step. For now, edit .data/settings.toml.",
                }
            }
            HomeItem::Quit => return Flow::Quit,
        }
        Flow::Continue
    }

    fn pause_choice(&mut self, item: PauseItem) -> Flow {
        match item {
            PauseItem::Resume => {
                self.overlays.pop();
            }
            PauseItem::Open => {
                self.leave_game();
                self.go_open();
            }
            PauseItem::New => {
                self.leave_game();
                self.screen = new_puzzle_placeholder();
            }
            PauseItem::Quit => {
                self.leave_game();
                self.go_home();
            }
            PauseItem::Restart => self.overlays.push(Overlay::Confirm(Confirm::restart())),
            PauseItem::Picture => {
                if let Screen::Game(game) = &mut self.screen {
                    game.picture = Some(self.settings.labels == crate::settings::Labels::Always);
                    let png = self.settings.picture_format == crate::settings::PictureFormat::Png;
                    self.overlays.push(Overlay::Picture { png });
                }
            }
            // Not built yet; the menu shows them disabled.
            PauseItem::Checkpoints | PauseItem::Solution => {}
        }
        Flow::Continue
    }

    pub fn go_home(&mut self) {
        self.overlays.clear();
        let has_last = matches!(library::load_last(&self.data_dir()), Ok(Some(_)));
        self.screen = Screen::Home(Menu::new(vec![
            item("Continue", Some('c'), has_last, HomeItem::Continue),
            item("Open a puzzle", Some('o'), true, HomeItem::Open),
            item("New puzzle", Some('n'), true, HomeItem::New),
            item("Settings", Some('s'), true, HomeItem::Settings),
            item("Quit", Some('q'), true, HomeItem::Quit),
        ]));
    }

    pub fn go_open(&mut self) {
        self.overlays.clear();
        let mut entries: Vec<Entry> = BUILT_IN
            .iter()
            .map(|(name, source)| Entry::BuiltIn {
                name,
                title: file::parse(source).map(|s| s.name).unwrap_or_else(|_| name.to_string()),
            })
            .collect();
        match library::list_puzzles(&self.data_dir()) {
            Ok(listed) => entries.extend(listed.into_iter().map(|l| match l.puzzle {
                Ok(spec) => Entry::File {
                    title: if spec.name.is_empty() { l.file.clone() } else { spec.name },
                    file: l.file,
                },
                Err(error) => Entry::Broken { file: l.file, error },
            })),
            Err(e) => self.message = Some(e),
        }
        let items = entries
            .iter()
            .enumerate()
            .map(|(i, e)| match e {
                Entry::BuiltIn { title, .. } => item(title.clone(), None, true, i),
                Entry::File { title, .. } => item(title.clone(), None, true, i),
                Entry::Broken { file, .. } => item(file.clone(), None, false, i),
            })
            .collect();
        self.screen = Screen::Open(Menu::new(items), entries);
    }

    fn open_entry(&mut self, entry: &Entry) {
        let opened = match entry {
            Entry::BuiltIn { name, .. } => {
                let source = BUILT_IN.iter().find(|(n, _)| n == name).map(|(_, s)| *s).unwrap_or_default();
                file::parse(source)
                    .map_err(|e| e.to_string())
                    .map(|spec| (spec, Source::BuiltIn { name: name.to_string() }))
            }
            Entry::File { file, .. } => {
                library::read_puzzle(&self.data_dir(), file).map(|spec| (spec, Source::File { file: file.clone() }))
            }
            Entry::Broken { error, .. } => Err(error.clone()),
        };
        match opened {
            Ok((spec, source)) => self.open_puzzle(spec, source),
            Err(e) => self.message = Some(e),
        }
    }

    fn continue_last(&mut self) {
        let last = match library::load_last(&self.data_dir()) {
            Ok(Some(last)) => last,
            Ok(None) => return,
            Err(e) => {
                self.message = Some(e);
                return;
            }
        };
        // Prefer the puzzle's own file, for its name and description; fall
        // back to the saved share code if the file has gone.
        let from_source = match &last.source {
            Source::BuiltIn { name } => BUILT_IN
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, s)| file::parse(s).ok()),
            Source::File { file } => library::read_puzzle(&self.data_dir(), file).ok(),
            Source::Code => None,
        };
        let spec = from_source
            .filter(|s| s.to_code().ok().as_deref() == Some(last.code.as_str()))
            .or_else(|| {
                PuzzleSpec::from_code(&last.code).ok().map(|mut s| {
                    s.name = last.name.clone();
                    s
                })
            });
        match spec {
            Some(spec) => {
                let source = if spec.to_code().ok().as_deref() == Some(last.code.as_str()) { last.source } else { Source::Code };
                self.open_puzzle(spec, source)
            }
            None => self.message = Some("could not reopen your last puzzle".into()),
        }
    }

    /// Opens a puzzle in the game screen, restoring any saved progress.
    pub fn open_puzzle(&mut self, spec: PuzzleSpec, source: Source) {
        match GameScreen::open(spec, source, &self.data_dir()) {
            Ok((game, warning)) => {
                self.overlays.clear();
                self.summary = None;
                self.summary_waiting = false;
                let last = LastPuzzle { name: game.title().to_string(), code: game.code.clone(), source: game.source.clone() };
                if let Err(e) = library::save_last(&self.data_dir(), &last) {
                    self.message = Some(e);
                }
                if warning.is_some() {
                    self.message = warning;
                }
                self.screen = Screen::Game(Box::new(game));
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Acts on the answer to a dialogue.
    fn answer(&mut self, confirm: &Confirm, yes: bool) {
        match confirm.question {
            Question::Hint => {
                if yes && confirm.dont_ask == Some(true) {
                    self.settings.confirm_hints = false;
                    if let Err(e) = self.settings.save(&self.home) {
                        self.message = Some(e);
                    }
                }
                if let (true, Screen::Game(game)) = (yes, &mut self.screen) {
                    game.give_hint(Instant::now());
                }
            }
            Question::Restart => {
                let Screen::Game(game) = &self.screen else { return };
                if !yes {
                    return;
                }
                if game.game().move_count() > 0 && !game.on_checkpoint() {
                    self.overlays.push(Overlay::Confirm(Confirm::keep_before_restart()));
                } else {
                    self.restart(false);
                }
            }
            Question::KeepBeforeRestart => self.restart(yes),
        }
    }

    /// Saves a picture of the open puzzle to `pictures/` in the data
    /// folder: the board as PNG, or the screen as ANSI text at the size it
    /// was last drawn. Returns to the game and says where it went.
    fn save_picture(&mut self, png: bool) {
        let Screen::Game(game) = &self.screen else { return };
        let labels = game.picture.unwrap_or(false);
        let slug: String = game
            .title()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect::<String>()
            .split('_')
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let ext = if png { "png" } else { "ans" };
        let dir = self.data_dir().join("pictures");
        let path = (1..)
            .map(|n| match n {
                1 => dir.join(format!("{slug}_{stamp}.{ext}")),
                n => dir.join(format!("{slug}_{stamp}_{n}.{ext}")),
            })
            .find(|p| !p.exists())
            .expect("a free name");
        let bytes = if png {
            game.picture_png(&self.settings, labels)
        } else {
            // The screen as it is behind the picture view.
            let overlays = std::mem::take(&mut self.overlays);
            let (w, h) = self.size.get();
            let text = run::snapshot(self, w, h);
            self.overlays = overlays;
            Ok(text.into_bytes())
        };
        let written = bytes.and_then(|b| {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::write(&path, b).map_err(|e| format!("{}: {e}", path.display()))
        });
        self.overlays.clear();
        if let Screen::Game(game) = &mut self.screen {
            game.picture = None;
        }
        self.message = Some(match written {
            Ok(()) => format!("Saved picture: {}", path.display()),
            Err(e) => format!("Could not save the picture: {e}"),
        });
    }

    /// The open puzzle's board as a PNG, for `dd snapshot --png`.
    pub fn board_png(&self, labels: bool) -> Result<Vec<u8>, String> {
        match &self.screen {
            Screen::Game(game) => game.picture_png(&self.settings, labels),
            _ => Err("no puzzle is open".into()),
        }
    }

    /// Restarts the open puzzle, first keeping the position as a checkpoint
    /// if asked, then saves and returns to the game.
    fn restart(&mut self, keep: bool) {
        let data_dir = self.data_dir();
        let Screen::Game(game) = &mut self.screen else { return };
        let kept = if keep { game.keep_checkpoint() } else { None };
        game.restart();
        let saved = game.save(&data_dir);
        self.overlays.clear();
        self.summary = None;
        self.summary_waiting = false;
        self.message = Some(match (saved, kept) {
            (Err(e), _) => e,
            (Ok(()), Some(n)) => format!("Restarted; your position is kept as checkpoint {n}"),
            (Ok(()), None) => "Restarted".into(),
        });
    }

    /// Records a solve of the open puzzle and says so in the footer.
    fn record_solve(&mut self) {
        let Screen::Game(game) = &self.screen else { return };
        let moves = game.game().move_count();
        let score = Score::new(moves, game.progress.counters);
        let (id, title) = (game.id(), game.title().to_string());
        let improvement = self.records.add_solve(&id, &title, score, self.settings.best);
        let data_dir = self.data_dir();
        let saved = self.records.save(&data_dir).and_then(|()| match &self.screen {
            Screen::Game(game) => game.save(&data_dir),
            _ => Ok(()),
        });
        self.message = Some(match saved {
            Err(e) => e,
            Ok(()) if improvement.shown => format!("{} · new best!", solved_message(&score)),
            Ok(()) => solved_message(&score),
        });
        self.summary = Some(Summary { score, new_best: improvement.shown });
        self.summary_waiting = true;
    }

    /// Saves progress if a game is open. The screen stays as it is.
    pub fn leave_game(&mut self) {
        if let Screen::Game(game) = &self.screen {
            if let Err(e) = game.save(&self.data_dir()) {
                self.message = Some(e);
            }
        }
    }
}

/// "Solved in 12 moves", with any help it had.
fn solved_message(score: &Score) -> String {
    let plural = |n: u32, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    let mut text = format!("Solved in {}", plural(score.moves, "move"));
    if score.hints > 0 {
        text.push_str(&format!(" with {}", plural(score.hints, "hint")));
    }
    if score.solution_viewed {
        text.push_str(", having viewed the solution");
    }
    text
}

fn pause_menu() -> Menu<PauseItem> {
    Menu::new(vec![
        item("Resume", None, true, PauseItem::Resume),
        item("Restart", Some('r'), true, PauseItem::Restart),
        not_yet("Checkpoints", Some('c'), PauseItem::Checkpoints),
        not_yet("View solution", Some('s'), PauseItem::Solution),
        item("Save picture", Some('p'), true, PauseItem::Picture),
        item("Open a puzzle", Some('o'), true, PauseItem::Open),
        item("New puzzle", Some('n'), true, PauseItem::New),
        item("Quit to home", Some('q'), true, PauseItem::Quit),
    ])
}

fn new_puzzle_placeholder() -> Screen {
    Screen::Placeholder { title: "New puzzle", note: "The puzzle editor arrives in a later version." }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::home::testing::TempDir;
    use crate::data::progress;
    use dd_engine::Dir;

    fn app() -> (TempDir, App) {
        let tmp = TempDir::new();
        let app = App::new(Home::at(&tmp.0)).unwrap();
        (tmp, app)
    }

    fn press(app: &mut App, keys: &[Key]) -> Flow {
        keys.iter().fold(Flow::Continue, |_, &k| app.handle(k))
    }

    fn game(app: &App) -> &GameScreen {
        match &app.screen {
            Screen::Game(g) => g,
            _ => panic!("not in a game"),
        }
    }

    #[test]
    fn starts_at_home_with_continue_disabled() {
        let (_tmp, app) = app();
        let Screen::Home(menu) = &app.screen else { panic!() };
        assert!(!menu.items[0].enabled);
        assert_eq!(menu.chosen(), Some(HomeItem::Open));
        assert_eq!(app.context(), Context::Home);
    }

    #[test]
    fn open_a_built_in_puzzle_and_quit_back_home() {
        let (tmp, mut app) = app();
        press(&mut app, &[Key::Char('o')]);
        assert!(matches!(app.screen, Screen::Open(..)));
        // Klotski is the second built-in.
        press(&mut app, &[Key::Arrow(Dir::Down), Key::Enter]);
        assert_eq!(game(&app).title(), "Klotski");
        assert_eq!(app.context(), Context::Game);

        // Pause, then quit to home: progress and the last puzzle are saved.
        press(&mut app, &[Key::Char('.')]);
        assert_eq!(app.context(), Context::Pause);
        press(&mut app, &[Key::Char('q')]);
        assert!(matches!(app.screen, Screen::Home(_)));
        let code = file::parse(BUILT_IN[1].1).unwrap().to_code().unwrap();
        assert!(progress::progress_file(&tmp.0, &code).exists());
        let Screen::Home(menu) = &app.screen else { panic!() };
        assert!(menu.items[0].enabled, "Continue is offered now");

        // Continue reopens it.
        press(&mut app, &[Key::Char('c')]);
        assert_eq!(game(&app).title(), "Klotski");
    }

    #[test]
    fn esc_closes_the_top_overlay_only() {
        let (_tmp, mut app) = app();
        press(&mut app, &[Key::Char('o'), Key::Enter, Key::Char('?')]);
        assert_eq!(app.context(), Context::Help);
        press(&mut app, &[Key::Esc]);
        assert_eq!(app.context(), Context::Game);
        press(&mut app, &[Key::Char('.'), Key::Esc]);
        assert_eq!(app.context(), Context::Game);
        // Disabled pause items do nothing.
        press(&mut app, &[Key::Char('.'), Key::Char('c')]);
        assert_eq!(app.context(), Context::Pause);
    }

    #[test]
    fn ctrl_c_quits_from_anywhere_and_saves() {
        let (tmp, mut app) = app();
        press(&mut app, &[Key::Char('o'), Key::Enter, Key::Char('.')]);
        assert_eq!(app.handle(Key::Interrupt), Flow::Quit);
        let code = file::parse(BUILT_IN[0].1).unwrap().to_code().unwrap();
        assert!(progress::progress_file(&tmp.0, &code).exists());
    }

    #[test]
    fn esc_on_open_goes_home_and_q_at_home_quits() {
        let (_tmp, mut app) = app();
        press(&mut app, &[Key::Char('o'), Key::Esc]);
        assert!(matches!(app.screen, Screen::Home(_)));
        assert_eq!(press(&mut app, &[Key::Char('q')]), Flow::Quit);
    }

    #[test]
    fn your_puzzles_are_listed_and_broken_ones_cannot_be_opened() {
        let (tmp, mut app) = app();
        let mut spec = file::parse(BUILT_IN[0].1).unwrap();
        spec.name = "Mine".into();
        library::save_puzzle(&tmp.0, &spec).unwrap();
        std::fs::write(tmp.0.join("puzzles/zz_broken.toml"), "grid = 1").unwrap();
        press(&mut app, &[Key::Char('o')]);
        let Screen::Open(menu, entries) = &app.screen else { panic!() };
        assert_eq!(entries.len(), BUILT_IN.len() + 2);
        assert!(menu.items[BUILT_IN.len()].enabled);
        assert!(!menu.items[BUILT_IN.len() + 1].enabled);
        // Up from the first item wraps to the last enabled one: "Mine".
        press(&mut app, &[Key::Arrow(Dir::Up), Key::Enter]);
        assert_eq!(game(&app).title(), "Mine");
    }

    #[test]
    fn an_unreadable_progress_file_is_never_overwritten() {
        let (tmp, mut app) = app();
        let code = file::parse(BUILT_IN[0].1).unwrap().to_code().unwrap();
        let path = progress::progress_file(&tmp.0, &code);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "garbage [").unwrap();
        press(&mut app, &[Key::Char('o'), Key::Enter]);
        assert!(app.message.as_deref().unwrap_or("").contains("not saving"));
        press(&mut app, &[Key::Char('.'), Key::Char('q')]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "garbage [");
    }

    fn tiny(app: &mut App) {
        let mut spec = PuzzleSpec::from_grid("A.\n..").unwrap();
        spec.name = "Tiny".into();
        spec.goals.push(dd_engine::GoalSpec { block: 'A', x: 1, y: 1 });
        app.open_puzzle(spec, Source::Code);
    }

    #[test]
    fn solving_records_a_best_and_says_so() {
        let (tmp, mut app) = app();
        tiny(&mut app);
        press(&mut app, &[Key::Char('s'), Key::ShiftArrow(Dir::Right), Key::ShiftArrow(Dir::Down)]);
        assert_eq!(app.message.as_deref(), Some("Solved in 1 move · new best!"));
        let records = Records::load(&tmp.0).unwrap();
        let record = records.get(&game(&app).id()).unwrap();
        assert_eq!(record.solves, 1);
        assert_eq!(record.best.map(|s| s.moves), Some(1));
        assert!(app.animating(), "the exit plays");

        // Solving again the same way is not a new best.
        press(&mut app, &[Key::Char('u'), Key::Char('s'), Key::ShiftArrow(Dir::Down), Key::ShiftArrow(Dir::Right)]);
        assert_eq!(app.message.as_deref(), Some("Solved in 1 move"));
    }

    #[test]
    fn solved_messages_read_naturally() {
        let score = |moves, hints, solution_viewed| Score { moves, hints, solution_viewed };
        assert_eq!(solved_message(&score(1, 0, false)), "Solved in 1 move");
        assert_eq!(solved_message(&score(85, 2, false)), "Solved in 85 moves with 2 hints");
        assert_eq!(solved_message(&score(81, 1, true)), "Solved in 81 moves with 1 hint, having viewed the solution");
    }

    #[test]
    fn goal_toggle_is_saved_and_help_follows_the_mode() {
        let (tmp, mut app) = app();
        tiny(&mut app);
        press(&mut app, &[Key::Char('g')]);
        assert!(app.settings.show_goal);
        assert!(Settings::load(&Home::at(&tmp.0)).unwrap().show_goal);
        press(&mut app, &[Key::Char('s'), Key::Char(' '), Key::Char('?')]);
        assert!(matches!(app.overlays.last(), Some(Overlay::Help(Context::Grab))));
        press(&mut app, &[Key::Esc]);
        assert_eq!(app.context(), Context::Grab, "closing help leaves the block held");
    }

    /// Ticks the app until the open puzzle's solver has finished.
    fn settle(app: &mut App) {
        for _ in 0..2000 {
            app.tick(Instant::now());
            if let Screen::Game(g) = &app.screen {
                if !matches!(g.solver, hints::SolverState::Running(_)) && g.hint_search.is_none() {
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("solver did not finish");
    }

    #[test]
    fn hints_ask_first_unless_told_not_to() {
        let (tmp, mut app) = app();
        press(&mut app, &[Key::Char('o'), Key::Enter]);
        // Before the solver has reported, a hint is refused with a reason.
        press(&mut app, &[Key::Char('h')]);
        assert!(app.message.as_deref().unwrap_or("").contains("still working"));
        settle(&mut app);

        // "No" closes the dialogue and gives nothing.
        press(&mut app, &[Key::Char('h')]);
        assert_eq!(app.context(), Context::Confirm);
        press(&mut app, &[Key::Char('n')]);
        assert_eq!(app.context(), Context::Game);
        assert!(game(&app).hint.is_none());

        // Tab moves the focus to No; Enter then declines too.
        press(&mut app, &[Key::Char('h'), Key::Tab, Key::Enter]);
        assert!(game(&app).hint.is_none());

        // Tick "don't ask again" and say yes: a hint, and no more asking.
        press(&mut app, &[Key::Char('h'), Key::Char(' '), Key::Char('y')]);
        assert!(game(&app).hint.is_some());
        assert_eq!(game(&app).progress.counters.hints, 1);
        assert!(!app.settings.confirm_hints);
        assert!(!Settings::load(&Home::at(&tmp.0)).unwrap().confirm_hints);

        // The next hint comes straight away, with no dialogue.
        press(&mut app, &[Key::Char('h')]);
        assert_eq!(app.context(), Context::Game);
        assert_eq!(game(&app).progress.counters.hints, 2);
    }

    #[test]
    fn restart_asks_then_offers_to_keep_the_position() {
        let (tmp, mut app) = app();
        tiny(&mut app);
        press(&mut app, &[Key::Char('s'), Key::ShiftArrow(Dir::Right), Key::Esc]);
        assert_eq!(game(&app).game().move_count(), 1);

        // No, by default: Enter on the restart question keeps playing.
        press(&mut app, &[Key::Char('.'), Key::Char('r'), Key::Enter]);
        assert_eq!(app.context(), Context::Pause);
        assert_eq!(game(&app).game().move_count(), 1);

        // Yes, then keep the position: it becomes checkpoint 1.
        press(&mut app, &[Key::Char('r'), Key::Char('y'), Key::Char('y')]);
        assert_eq!(app.context(), Context::Game);
        let g = game(&app);
        assert_eq!(g.game().move_count(), 0);
        assert_eq!(g.progress.checkpoints.len(), 1);
        assert_eq!(g.progress.checkpoints[0].number, 1);
        assert_eq!(app.message.as_deref(), Some("Restarted; your position is kept as checkpoint 1"));
        let code = game(&app).code.clone();
        let puzzle = game(&app).game().puzzle().clone();
        let saved = progress::load(&tmp.0, &code, &puzzle).unwrap().unwrap();
        assert_eq!(saved.checkpoints.len(), 1);
        assert_eq!(saved.game.move_count(), 0);

        // With no moves there is nothing to keep: one question only.
        press(&mut app, &[Key::Char('.'), Key::Char('r'), Key::Char('y')]);
        assert_eq!(app.context(), Context::Game);
        assert_eq!(app.message.as_deref(), Some("Restarted"));
    }

    #[test]
    fn restart_resets_hints_and_does_not_keep_a_route_twice() {
        let (_tmp, mut app) = app();
        tiny(&mut app);
        settle(&mut app);
        press(&mut app, &[Key::Char('h'), Key::Char('y')]);
        assert_eq!(game(&app).progress.counters.hints, 1);
        press(&mut app, &[Key::Char('s'), Key::ShiftArrow(Dir::Right), Key::Esc]);
        press(&mut app, &[Key::Char('.'), Key::Char('r'), Key::Char('y'), Key::Char('y')]);
        assert_eq!(game(&app).progress.counters.hints, 0);
        assert_eq!(game(&app).progress.checkpoints[0].counters.hints, 1, "the checkpoint keeps its count");

        // Play the same move again: the route is already a checkpoint, so
        // restarting asks only once.
        press(&mut app, &[Key::Char('s'), Key::ShiftArrow(Dir::Right), Key::Esc]);
        press(&mut app, &[Key::Char('.'), Key::Char('r'), Key::Char('y')]);
        assert_eq!(app.context(), Context::Game);
        assert_eq!(game(&app).progress.checkpoints.len(), 1);
    }

    #[test]
    fn preferences_change_live_and_are_saved() {
        let (tmp, mut app) = app();
        tiny(&mut app);
        press(&mut app, &[Key::Char(',')]);
        assert_eq!(app.context(), Context::Preferences);
        // Theme is the first row: right moves to the next theme.
        press(&mut app, &[Key::Arrow(Dir::Right)]);
        assert_eq!(app.settings.theme, "escape-light");
        // Down to grain (escape-light has texture) and again to the goal.
        press(&mut app, &[Key::Arrow(Dir::Down), Key::Arrow(Dir::Down), Key::Enter]);
        assert!(app.settings.show_goal);
        let saved = Settings::load(&Home::at(&tmp.0)).unwrap();
        assert_eq!((saved.theme.as_str(), saved.show_goal), ("escape-light", true));
        press(&mut app, &[Key::Char(',')]);
        assert_eq!(app.context(), Context::Game);
    }

    /// Ticks the app forward through simulated time until `done` holds.
    fn run_until(app: &mut App, done: impl Fn(&App) -> bool) {
        let mut now = Instant::now();
        for _ in 0..2000 {
            app.tick(now);
            if done(app) {
                return;
            }
            now += std::time::Duration::from_millis(50);
        }
        panic!("never got there");
    }

    #[test]
    fn the_summary_follows_the_fade_and_replay_returns_to_it() {
        let (_tmp, mut app) = app();
        tiny(&mut app);
        press(&mut app, &[Key::Char('s'), Key::ShiftArrow(Dir::Right), Key::ShiftArrow(Dir::Down)]);
        assert!(app.overlays.is_empty(), "the summary waits for the fade");
        run_until(&mut app, |a| a.context() == Context::Summary);
        let Some(Overlay::Summary(summary)) = app.overlays.last() else { panic!() };
        assert_eq!(summary.score.moves, 1);
        assert!(summary.new_best);

        // v looks at the board.
        press(&mut app, &[Key::Char('v')]);
        assert_eq!(app.context(), Context::Game);

        // Undo and solve again, then replay: the replay plays the moves and
        // brings the summary back.
        press(&mut app, &[Key::Char('u'), Key::Char('s'), Key::ShiftArrow(Dir::Right), Key::ShiftArrow(Dir::Down)]);
        run_until(&mut app, |a| a.context() == Context::Summary);
        press(&mut app, &[Key::Char('p')]);
        assert_eq!(app.context(), Context::Replay);
        assert_eq!(game(&app).shown().move_count(), 0, "the replay starts from the beginning");
        run_until(&mut app, |a| a.context() == Context::Summary);
        assert!(game(&app).replay.is_none());

        // Stopping a replay early also brings it back.
        press(&mut app, &[Key::Char('p'), Key::Esc]);
        run_until(&mut app, |a| a.context() == Context::Summary);

        // o leaves for the Open screen.
        press(&mut app, &[Key::Char('o')]);
        assert!(matches!(app.screen, Screen::Open(..)));
    }

    #[test]
    fn restarting_after_a_solve_drops_the_summary() {
        let (_tmp, mut app) = app();
        tiny(&mut app);
        press(&mut app, &[Key::Char('s'), Key::ShiftArrow(Dir::Right), Key::ShiftArrow(Dir::Down)]);
        press(&mut app, &[Key::Char('.'), Key::Char('r'), Key::Char('y'), Key::Char('n')]);
        settle(&mut app);
        run_until(&mut app, |a| !a.animating());
        assert_eq!(app.context(), Context::Game);
        assert!(app.summary.is_none());
    }

    #[test]
    fn pictures_save_as_png_or_ansi_and_say_where() {
        let (tmp, mut app) = app();
        tiny(&mut app);
        press(&mut app, &[Key::Char('.'), Key::Char('p')]);
        assert_eq!(app.context(), Context::Picture);
        assert_eq!(game(&app).picture, Some(false), "labels start off unless always shown");
        press(&mut app, &[Key::Char('j')]);
        assert_eq!(game(&app).picture, Some(true));
        press(&mut app, &[Key::Enter]);
        assert_eq!(app.context(), Context::Game);
        assert!(game(&app).picture.is_none());
        let message = app.message.clone().unwrap();
        let path = message.strip_prefix("Saved picture: ").expect(&message);
        assert!(path.ends_with(".png"));
        let name = std::path::Path::new(path).file_name().unwrap().to_string_lossy().into_owned();
        // tiny_YYYYMMDD_HHmmss.png
        assert!(name.starts_with("tiny_") && name.len() == "tiny_20261128_153205.png".len(), "{name}");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[1..4], b"PNG");
        assert!(path.starts_with(&tmp.0.join("pictures").to_string_lossy().into_owned()));

        // a switches to ANSI: the whole screen as colour text.
        press(&mut app, &[Key::Char('.'), Key::Char('p'), Key::Char('a'), Key::Enter]);
        let message = app.message.clone().unwrap();
        let path = message.strip_prefix("Saved picture: ").expect(&message);
        assert!(path.ends_with(".ans"));
        let text = std::fs::read_to_string(path).unwrap();
        let plain: String = text.split('\x1b').map(|p| p.split_once('m').map_or(p, |(_, rest)| rest)).collect();
        assert!(text.contains("\x1b[") && plain.contains("Tiny"));

        // Esc backs out to the pause menu without saving.
        press(&mut app, &[Key::Char('.'), Key::Char('p'), Key::Esc]);
        assert_eq!(app.context(), Context::Pause);
        assert_eq!(std::fs::read_dir(tmp.0.join("pictures")).unwrap().count(), 2);
    }

    #[test]
    fn the_picture_view_bar_and_ansi_note() {
        let (_tmp, mut app) = app();
        tiny(&mut app);
        press(&mut app, &[Key::Char('.'), Key::Char('p')]);
        let text = run::snapshot(&app, 100, 24);
        let plain: String = text.split('\x1b').map(|p| p.split_once('m').map_or(p, |(_, rest)| rest)).collect();
        assert!(plain.contains("PNG picture") && plain.contains("a ANSI"), "{plain}");
        assert!(!plain.contains("Paused"), "the pause box is hidden in the picture view");
        press(&mut app, &[Key::Char('a')]);
        let text = run::snapshot(&app, 120, 24);
        let plain: String = text.split('\x1b').map(|p| p.split_once('m').map_or(p, |(_, rest)| rest)).collect();
        assert!(plain.contains("ANSI picture") && plain.contains("cat the file"), "{plain}");
    }
}
