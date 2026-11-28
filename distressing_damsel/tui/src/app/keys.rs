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
    /// Base mode to selection mode.
    Select,
    /// Into jump mode.
    Jump,
    /// A block label typed in jump mode.
    Label(char),
    /// Move the cursor, in selection mode.
    Move(Dir),
    /// Slide a block one cell: the held block, or with Shift the block
    /// under the cursor.
    Slide(Dir),
    Grab,
    Drop,
    Undo,
    Redo,
    ToggleGoal,
    Hint,
    Preferences,
    /// Change the chosen preference: forwards, or back.
    Adjust(bool),
    /// Play the player's solution back.
    Replay,
    /// Leave this puzzle for the Open screen.
    OpenAnother,
    // Dialogues.
    Yes,
    No,
    /// Move between a dialogue's choices.
    NextChoice,
    /// Tick or untick "don't ask again".
    ToggleDontAsk,
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
    /// The game with no cursor.
    Game,
    /// The game with a cursor on a block.
    Select,
    /// The game with a block held.
    Grab,
    /// The game waiting for a block label.
    Jump,
    Pause,
    Help,
    /// A yes-or-no dialogue.
    Confirm,
    Preferences,
    /// The summary after solving.
    Summary,
    /// The player's solution playing back.
    Replay,
}

pub fn table(context: Context) -> &'static [Binding] {
    match context {
        Context::Home => HOME,
        Context::List => MENU,
        Context::Placeholder => PLACEHOLDER,
        Context::Game => GAME,
        Context::Select => SELECT,
        Context::Grab => GRAB,
        Context::Jump => JUMP,
        Context::Pause => PAUSE,
        Context::Help => HELP,
        Context::Confirm => CONFIRM,
        Context::Preferences => PREFERENCES,
        Context::Summary => SUMMARY,
        Context::Replay => REPLAY,
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

const ARROWS: &[Key] = &[Key::Arrow(Dir::Up), Key::Arrow(Dir::Down), Key::Arrow(Dir::Left), Key::Arrow(Dir::Right)];
const SHIFT_ARROWS: &[Key] =
    &[Key::ShiftArrow(Dir::Up), Key::ShiftArrow(Dir::Down), Key::ShiftArrow(Dir::Left), Key::ShiftArrow(Dir::Right)];

const GAME: &[Binding] = &[
    bind(&[Key::Char('s')], "s", "select a block", Action::Select, true),
    bind(&[Key::Char('j')], "j", "jump to a block", Action::Jump, true),
    bind(&[Key::Char('u')], "u", "undo", Action::Undo, true),
    bind(&[Key::Char('r')], "r", "redo", Action::Redo, false),
    bind(&[Key::Char('g')], "g", "goal outline on/off", Action::ToggleGoal, false),
    bind(&[Key::Char('h')], "h", "hint", Action::Hint, false),
    bind(&[Key::Char(',')], ",", "preferences", Action::Preferences, false),
    bind(&[Key::Char('?')], "?", "keys", Action::Help, true),
    bind(&[Key::Char('.')], ".", "pause menu", Action::Pause, true),
];

const SELECT: &[Binding] = &[
    bind(ARROWS, "arrows", "move the cursor", Action::Move(Dir::Up), true),
    bind(SHIFT_ARROWS, "shift+arrows", "slide the block", Action::Slide(Dir::Up), true),
    bind(&[Key::Char(' ')], "space", "grab the block", Action::Grab, true),
    bind(&[Key::Char('j')], "j", "jump to a block", Action::Jump, false),
    bind(&[Key::Char('u')], "u", "undo", Action::Undo, false),
    bind(&[Key::Char('r')], "r", "redo", Action::Redo, false),
    bind(&[Key::Char('g')], "g", "goal outline on/off", Action::ToggleGoal, false),
    bind(&[Key::Char('h')], "h", "hint", Action::Hint, false),
    bind(&[Key::Char(',')], ",", "preferences", Action::Preferences, false),
    bind(&[Key::Esc], "esc", "leave selection", Action::Back, true),
    bind(&[Key::Char('?')], "?", "keys", Action::Help, true),
    bind(&[Key::Char('.')], ".", "pause menu", Action::Pause, false),
];

const GRAB: &[Binding] = &[
    bind(ARROWS, "arrows", "slide the block", Action::Slide(Dir::Up), true),
    bind(&[Key::Char(' '), Key::Enter], "space", "drop it", Action::Drop, true),
    bind(&[Key::Esc], "esc", "put it back", Action::Back, true),
    bind(&[Key::Char('?')], "?", "keys", Action::Help, true),
];

const JUMP: &[Binding] = &[
    bind(&[], "a label", "jump to that block", Action::Label(' '), true),
    bind(&[Key::Esc], "esc", "cancel", Action::Back, true),
];

const PAUSE: &[Binding] = &[
    bind(&[Key::Esc, Key::Char('.')], "esc .", "resume", Action::Resume, true),
    bind(&[UP, Key::BackTab], "↑ ↓", "choose", Action::Up, true),
    bind(&[DOWN, Key::Tab], "", "", Action::Down, false),
    bind(&[Key::Enter], "enter", "pick", Action::Choose, true),
];

const CONFIRM: &[Binding] = &[
    bind(&[Key::Char('y')], "y", "yes", Action::Yes, true),
    bind(&[Key::Char('n'), Key::Esc], "n", "no", Action::No, true),
    bind(&[Key::Tab, Key::BackTab, Key::Arrow(Dir::Left), Key::Arrow(Dir::Right)], "tab", "switch", Action::NextChoice, true),
    bind(&[Key::Enter], "enter", "choose", Action::Choose, true),
    bind(&[Key::Char(' ')], "space", "don't ask again", Action::ToggleDontAsk, false),
];

const PREFERENCES: &[Binding] = &[
    bind(&[UP, Key::BackTab], "↑ ↓", "choose", Action::Up, true),
    bind(&[DOWN, Key::Tab], "", "", Action::Down, false),
    bind(&[Key::Arrow(Dir::Right), Key::Enter, Key::Char(' ')], "← →", "change", Action::Adjust(true), true),
    bind(&[Key::Arrow(Dir::Left)], "", "", Action::Adjust(false), false),
    bind(&[Key::Esc, Key::Char(',')], "esc", "close", Action::Back, true),
];

const SUMMARY: &[Binding] = &[
    bind(&[Key::Char('v'), Key::Esc], "v", "look at the board", Action::Back, true),
    bind(&[Key::Char('p')], "p", "replay your solution", Action::Replay, true),
    bind(&[Key::Char('o')], "o", "open another puzzle", Action::OpenAnother, true),
];

const REPLAY: &[Binding] = &[bind(&[Key::Esc, Key::Char('p'), Key::Char(' ')], "esc", "stop the replay", Action::Back, true)];

const HELP: &[Binding] = &[bind(&[Key::Esc, Key::Char('?'), Key::Enter], "esc", "close", Action::Back, true)];

/// The action `key` triggers in `context`, if any. Letters not bound
/// directly are offered as menu shortcuts in menus.
pub fn lookup(context: Context, key: Key) -> Option<Action> {
    if key == Key::Interrupt {
        return Some(Action::Quit);
    }
    let bound = table(context).iter().find(|b| b.keys.contains(&key)).map(|b| b.action);
    // Arrow bindings carry the direction of the arrow pressed.
    let bound = match (bound, key) {
        (Some(Action::Move(_)), Key::Arrow(d)) => Some(Action::Move(d)),
        (Some(Action::Slide(_)), Key::Arrow(d) | Key::ShiftArrow(d)) => Some(Action::Slide(d)),
        (other, _) => other,
    };
    match (bound, key, context) {
        (Some(action), _, _) => Some(action),
        (None, Key::Char(c), Context::Home | Context::Pause) => Some(Action::Shortcut(c)),
        (None, Key::Char(c), Context::Jump) if c.is_ascii_alphanumeric() => Some(Action::Label(c)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_key_is_bound_twice_in_a_context() {
        for context in [
            Context::Home,
            Context::List,
            Context::Placeholder,
            Context::Game,
            Context::Select,
            Context::Grab,
            Context::Jump,
            Context::Pause,
            Context::Help,
            Context::Confirm,
            Context::Preferences,
            Context::Summary,
            Context::Replay,
        ] {
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

    #[test]
    fn arrows_carry_their_direction() {
        assert_eq!(lookup(Context::Select, Key::Arrow(Dir::Left)), Some(Action::Move(Dir::Left)));
        assert_eq!(lookup(Context::Select, Key::ShiftArrow(Dir::Down)), Some(Action::Slide(Dir::Down)));
        assert_eq!(lookup(Context::Grab, Key::Arrow(Dir::Right)), Some(Action::Slide(Dir::Right)));
        assert_eq!(lookup(Context::Game, Key::Arrow(Dir::Right)), None);
    }

    #[test]
    fn jump_mode_takes_labels_and_nothing_else() {
        assert_eq!(lookup(Context::Jump, Key::Char('B')), Some(Action::Label('B')));
        assert_eq!(lookup(Context::Jump, Key::Char('7')), Some(Action::Label('7')));
        assert_eq!(lookup(Context::Jump, Key::Char('.')), None);
        assert_eq!(lookup(Context::Jump, Key::Esc), Some(Action::Back));
    }
}
