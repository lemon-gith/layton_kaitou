//! The terminal side: reading keys and drawing frames.

use std::io;
use std::time::Duration;

use dd_engine::Dir;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;

use super::keys::Key;
use super::{App, Flow};

/// Frame time while something is animating (60 fps)
const FRAME: Duration = Duration::from_millis(16);

/// Translates a terminal key event. Key releases and unbound keys give
/// `None`; held keys repeat.
pub fn key_from(event: KeyEvent) -> Option<Key> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);
    let arrow = |dir| Some(if shift { Key::ShiftArrow(dir) } else { Key::Arrow(dir) });
    match event.code {
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => Some(Key::Interrupt),
        _ if event.modifiers.contains(KeyModifiers::CONTROL) => None,
        KeyCode::Char(c) => Some(Key::Char(c)),
        KeyCode::Up => arrow(Dir::Up),
        KeyCode::Down => arrow(Dir::Down),
        KeyCode::Left => arrow(Dir::Left),
        KeyCode::Right => arrow(Dir::Right),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Esc => Some(Key::Esc),
        KeyCode::Tab => Some(Key::Tab),
        KeyCode::BackTab => Some(Key::BackTab),
        KeyCode::Backspace => Some(Key::Backspace),
        _ => None,
    }
}

/// Runs the app in the terminal until it quits. The terminal is restored
/// afterwards, and on a panic.
pub fn run(app: &mut App) -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let result = (|| loop {
        app.tick(std::time::Instant::now());
        terminal.draw(|frame| app.draw(frame))?;
        // Block until a key arrives, unless something is moving.
        if app.animating() && !event::poll(FRAME)? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key_from(key).is_some_and(|key| app.handle(key) == Flow::Quit) {
                return Ok(());
            }
        }
        // A resize needs nothing more: the next draw uses the new size.
    })();
    ratatui::restore();
    result
}

/// Reads a key from its name, as used by `dd snapshot`: `up`, `S-left`,
/// `enter`, `esc`, `tab`, `space`, or a single character.
pub fn key_named(name: &str) -> Option<Key> {
    let dir = |s: &str| match s {
        "up" => Some(Dir::Up),
        "down" => Some(Dir::Down),
        "left" => Some(Dir::Left),
        "right" => Some(Dir::Right),
        _ => None,
    };
    if let Some(rest) = name.strip_prefix("S-") {
        return dir(rest).map(Key::ShiftArrow);
    }
    if let Some(d) = dir(name) {
        return Some(Key::Arrow(d));
    }
    match name {
        "enter" => Some(Key::Enter),
        "esc" => Some(Key::Esc),
        "tab" => Some(Key::Tab),
        "backtab" => Some(Key::BackTab),
        "backspace" => Some(Key::Backspace),
        "space" => Some(Key::Char(' ')),
        "comma" => Some(Key::Char(',')),
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(Key::Char(c)),
                _ => None,
            }
        }
    }
}

/// Draws the app at `width` by `height` without a terminal, as 24-bit ANSI
/// text: for screenshots and checks.
pub fn snapshot(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let rgb = |c: Color, default: (u8, u8, u8)| match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => default,
    };
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buffer[(x, y)];
            let (fr, fg, fb) = rgb(cell.fg, (204, 204, 204));
            let (br, bg, bb) = rgb(cell.bg, (0, 0, 0));
            let bold = if cell.modifier.contains(Modifier::BOLD) { "1;" } else { "" };
            out.push_str(&format!("\x1b[0;{bold}38;2;{fr};{fg};{fb};48;2;{br};{bg};{bb}m{}", cell.symbol()));
        }
        out.push_str("\x1b[0m\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn terminal_keys_translate() {
        assert_eq!(key_from(press(KeyCode::Char('q'), KeyModifiers::NONE)), Some(Key::Char('q')));
        assert_eq!(key_from(press(KeyCode::Char('Q'), KeyModifiers::SHIFT)), Some(Key::Char('Q')));
        assert_eq!(key_from(press(KeyCode::Char('c'), KeyModifiers::CONTROL)), Some(Key::Interrupt));
        assert_eq!(key_from(press(KeyCode::Char('x'), KeyModifiers::CONTROL)), None);
        assert_eq!(key_from(press(KeyCode::Left, KeyModifiers::NONE)), Some(Key::Arrow(Dir::Left)));
        assert_eq!(key_from(press(KeyCode::Left, KeyModifiers::SHIFT)), Some(Key::ShiftArrow(Dir::Left)));
        let mut release = press(KeyCode::Enter, KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert_eq!(key_from(release), None);
        let mut repeat = press(KeyCode::Down, KeyModifiers::NONE);
        repeat.kind = KeyEventKind::Repeat;
        assert_eq!(key_from(repeat), Some(Key::Arrow(Dir::Down)));
    }

    #[test]
    fn key_names_parse() {
        assert_eq!(key_named("S-up"), Some(Key::ShiftArrow(Dir::Up)));
        assert_eq!(key_named("right"), Some(Key::Arrow(Dir::Right)));
        assert_eq!(key_named("space"), Some(Key::Char(' ')));
        assert_eq!(key_named("."), Some(Key::Char('.')));
        assert_eq!(key_named("nope"), None);
    }
}
