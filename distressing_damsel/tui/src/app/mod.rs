//! The interactive app: one screen, a stack of overlays on top, and keys
//! dispatched through the tables in [`keys`].
//!
//! `App::handle` takes a key and updates the state; `App::draw` paints it.
//! Neither touches the terminal, so both are tested directly. Loading and
//! saving go through [`crate::data`] in the app's [`Home`] folder.

// Wired into the game screen in the next commit.
#[allow(dead_code)]
pub mod cursor;
pub mod draw;
pub mod game;
pub mod keys;
pub mod menu;
pub mod run;

use dd_engine::{file, PuzzleSpec};

use crate::data::library::{self, LastPuzzle, Source};
use crate::data::records::Records;
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
}

pub struct App {
    pub home: Home,
    pub settings: Settings,
    pub records: Records,
    pub screen: Screen,
    pub overlays: Vec<Overlay>,
    /// Shown in the footer until the next key.
    pub message: Option<String>,
}

impl App {
    pub fn new(home: Home) -> Result<App, String> {
        let settings = Settings::load(&home)?;
        let data_dir = settings.data_dir(&home);
        let (records, message) = match Records::load(&data_dir) {
            Ok(records) => (records, None),
            Err(e) => (Records::default(), Some(format!("could not read your records: {e}"))),
        };
        let mut app = App { home, settings, records, screen: Screen::Placeholder { title: "", note: "" }, overlays: Vec::new(), message };
        app.go_home();
        Ok(app)
    }

    fn data_dir(&self) -> std::path::PathBuf {
        self.settings.data_dir(&self.home)
    }

    /// Whether something is moving and the screen needs redrawing without a
    /// key press.
    pub fn animating(&self) -> bool {
        false
    }

    /// The context whose keys are live.
    pub fn context(&self) -> Context {
        match self.overlays.last() {
            Some(Overlay::Help(_)) => Context::Help,
            Some(Overlay::Pause(_)) => Context::Pause,
            None => match &self.screen {
                Screen::Home(_) => Context::Home,
                Screen::Open(..) => Context::List,
                Screen::Game(_) => Context::Game,
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
            Screen::Game(_) => match action {
                Action::Help => self.overlays.push(Overlay::Help(Context::Game)),
                Action::Pause => self.overlays.push(Overlay::Pause(pause_menu())),
                _ => {}
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
            // Not built yet; the menu shows them disabled.
            PauseItem::Restart | PauseItem::Checkpoints | PauseItem::Solution => {}
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

    /// Saves progress if a game is open. The screen stays as it is.
    pub fn leave_game(&mut self) {
        if let Screen::Game(game) = &self.screen {
            if let Err(e) = game.save(&self.data_dir()) {
                self.message = Some(e);
            }
        }
    }
}

fn pause_menu() -> Menu<PauseItem> {
    Menu::new(vec![
        item("Resume", None, true, PauseItem::Resume),
        not_yet("Restart", Some('r'), PauseItem::Restart),
        not_yet("Checkpoints", Some('c'), PauseItem::Checkpoints),
        not_yet("View solution", Some('s'), PauseItem::Solution),
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
        press(&mut app, &[Key::Char('.'), Key::Char('r')]);
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
}
