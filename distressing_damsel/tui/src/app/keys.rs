//! Keys and what they do in each context.
//!
//! Every context has one table of bindings. The input handler looks keys up
//! in it, and the help overlay lists it, so the help cannot drift out of
//! date.

use dd_engine::Dir;

/// A key press, independent of the terminal library.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Arrow(Dir),
    ShiftArrow(Dir),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    /// Ctrl+C: always quits, saving first.
    Interrupt,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Action {
    // Menus and lists.
    Up,
    Down,
    Choose,
    /// Pick a menu item by its letter.
    Shortcut(char),
    Back,
    Quit,
    // The game.
    Help,
    Pause,
    // The pause menu.
    Resume,
}

pub struct Binding {
    pub keys: &'static [Key],
    /// How the keys are written in help and the footer.
    pub shown: &'static str,
    pub help: &'static str,
    pub action: Action,
    /// Listed in the footer as well as in help.
    pub footer: bool,
}

const fn bind(keys: &'static [Key], shown: &'static str, help: &'static str, action: Action, footer: bool) -> Binding {
    Binding { keys, shown, help, action, footer }
}

const UP: Key = Key::Arrow(Dir::Up);
const DOWN: Key = Key::Arrow(Dir::Down);

/// The contexts that have their own keys.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Context {
    Home,
    List,
    Placeholder,
    Game,
    Pause,
    Help,
}

pub fn table(context: Context) -> &'static [Binding] {
    match context {
        Context::Home => HOME,
        Context::List => MENU,
        Context::Placeholder => PLACEHOLDER,
        Context::Game => GAME,
        Context::Pause => PAUSE,
        Context::Help => HELP,
    }
}

const MENU: &[Binding] = &[
    bind(&[UP, Key::BackTab], "↑ ↓", "choose", Action::Up, true),
    bind(&[DOWN, Key::Tab], "", "", Action::Down, false),
    bind(&[Key::Enter], "enter", "open", Action::Choose, true),
    bind(&[Key::Esc], "esc", "back", Action::Back, true),
];

const HOME: &[Binding] = &[
    bind(&[UP, Key::BackTab], "↑ ↓", "choose", Action::Up, true),
    bind(&[DOWN, Key::Tab], "", "", Action::Down, false),
    bind(&[Key::Enter], "enter", "open", Action::Choose, true),
];

const PLACEHOLDER: &[Binding] = &[bind(&[Key::Esc, Key::Enter], "esc", "back", Action::Back, true)];

const GAME: &[Binding] = &[
    bind(&[Key::Char('?')], "?", "keys", Action::Help, true),
    bind(&[Key::Char('.')], ".", "pause menu", Action::Pause, true),
];

const PAUSE: &[Binding] = &[
    bind(&[Key::Esc, Key::Char('.')], "esc .", "resume", Action::Resume, true),
    bind(&[UP, Key::BackTab], "↑ ↓", "choose", Action::Up, true),
    bind(&[DOWN, Key::Tab], "", "", Action::Down, false),
    bind(&[Key::Enter], "enter", "pick", Action::Choose, true),
];

const HELP: &[Binding] = &[bind(&[Key::Esc, Key::Char('?'), Key::Enter], "esc", "close", Action::Back, true)];

/// The action `key` triggers in `context`, if any. Letters not bound
/// directly are offered as menu shortcuts in menus.
pub fn lookup(context: Context, key: Key) -> Option<Action> {
    if key == Key::Interrupt {
        return Some(Action::Quit);
    }
    let bound = table(context).iter().find(|b| b.keys.contains(&key)).map(|b| b.action);
    match (bound, key, context) {
        (Some(action), _, _) => Some(action),
        (None, Key::Char(c), Context::Home | Context::Pause) => Some(Action::Shortcut(c)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_key_is_bound_twice_in_a_context() {
        for context in [Context::Home, Context::List, Context::Placeholder, Context::Game, Context::Pause, Context::Help] {
            let keys: Vec<Key> = table(context).iter().flat_map(|b| b.keys.iter().copied()).collect();
            for (i, k) in keys.iter().enumerate() {
                assert!(!keys[i + 1..].contains(k), "{k:?} bound twice in {context:?}");
            }
        }
    }

    #[test]
    fn interrupt_always_quits_and_menus_take_letters() {
        assert_eq!(lookup(Context::Game, Key::Interrupt), Some(Action::Quit));
        assert_eq!(lookup(Context::Help, Key::Interrupt), Some(Action::Quit));
        assert_eq!(lookup(Context::Pause, Key::Char('q')), Some(Action::Shortcut('q')));
        assert_eq!(lookup(Context::Pause, Key::Char('.')), Some(Action::Resume));
        assert_eq!(lookup(Context::Game, Key::Char('z')), None);
    }
}
