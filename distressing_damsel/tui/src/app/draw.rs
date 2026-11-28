//! Drawing the app: screens, overlays and the footer.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use super::keys::{self, Context};
use super::menu::Menu;
use super::{App, Entry, Overlay, Screen};
use crate::look::{Rgb, Theme};
use crate::ui::colour;

/// Colours for text and menus, taken from the theme.
pub struct Chrome {
    pub background: Rgb,
    pub ink: Rgb,
    pub quiet: Rgb,
    pub accent: Rgb,
    pub warning: Rgb,
}

impl Chrome {
    pub fn new(theme: &Theme) -> Chrome {
        let background = theme.backdrop;
        let ink = if theme.dark { theme.text } else { theme.text_on_light };
        Chrome { background, ink, quiet: ink.mix(background, 0.5), accent: theme.accent, warning: theme.goal }
    }

    fn base(&self) -> Style {
        Style::new().bg(colour(self.background))
    }

    pub fn text(&self) -> Style {
        self.base().fg(colour(self.ink))
    }

    pub fn dim(&self) -> Style {
        self.base().fg(colour(self.quiet))
    }

    pub fn title(&self) -> Style {
        self.text().add_modifier(Modifier::BOLD)
    }

    pub fn selected(&self) -> Style {
        self.base().fg(colour(self.accent)).add_modifier(Modifier::BOLD)
    }

    pub fn warn(&self) -> Style {
        self.base().fg(colour(self.warning))
    }
}

impl App {
    pub fn draw(&self, frame: &mut Frame) {
        let chrome = Chrome::new(self.settings.theme());
        let area = frame.area();
        self.size.set((area.width, area.height));
        frame.render_widget(Block::new().style(chrome.text()), area);
        let main = Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), area.height.saturating_sub(1));
        let footer = Rect::new(area.x + 1, area.y + area.height.saturating_sub(1), area.width.saturating_sub(2), 1);

        match &self.screen {
            Screen::Home(menu) => draw_home(frame, main, menu, &chrome),
            Screen::Open(menu, entries) => draw_open(frame, main, menu, entries, &chrome),
            Screen::Game(game) => {
                let inner = Rect::new(main.x + 1, main.y + 1, main.width.saturating_sub(2), main.height.saturating_sub(1));
                game.draw(frame, inner, &self.settings, &self.records, &chrome)
            }
            Screen::Placeholder { title, note } => draw_placeholder(frame, main, title, note, &chrome),
        }
        // The picture view shows the screen as it will be saved: no other
        // overlays, a bar of its own, and for PNG all but the board dimmed.
        if let Some(Overlay::Picture { png }) = self.overlays.last() {
            if let (true, Screen::Game(game)) = (*png, &self.screen) {
                let inner = Rect::new(main.x + 1, main.y + 1, main.width.saturating_sub(2), main.height.saturating_sub(1));
                let board = game.arrange(&self.settings, inner).board;
                dim_outside(frame, main, board, &chrome);
            }
            draw_picture_bar(frame, footer, *png, &self.settings, &chrome);
            return;
        }
        for overlay in &self.overlays {
            match overlay {
                Overlay::Help(context) => draw_help(frame, main, *context, &chrome),
                Overlay::Pause(menu) => {
                    let lines = menu_lines(menu, &chrome, true);
                    draw_box(frame, main, " Paused ", lines, &chrome);
                }
                Overlay::Confirm(confirm) => draw_confirm(frame, main, confirm, &chrome),
                Overlay::Preferences(row) => draw_preferences(frame, main, *row, &self.settings, &chrome),
                Overlay::Picture { .. } => {}
                Overlay::Summary(summary) => {
                    let shortest = match &self.screen {
                        Screen::Game(game) => game.solver.shortest(),
                        _ => None,
                    };
                    draw_summary(frame, main, summary, shortest, &chrome);
                }
            }
        }
        draw_footer(frame, footer, self, &chrome);
    }
}

/// One line per item, the selected one marked; disabled items dimmed and
/// marked "not yet". With `letters`, each item's shortcut is shown.
fn menu_lines<'a, T: Copy>(menu: &Menu<T>, chrome: &Chrome, letters: bool) -> Vec<Line<'a>> {
    menu.items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let chosen = i == menu.selected;
            let style = if !item.enabled {
                chrome.dim()
            } else if chosen {
                chrome.selected()
            } else {
                chrome.text()
            };
            let mut spans = vec![Span::styled(if chosen { "▸ " } else { "  " }, style), Span::styled(item.label.clone(), style)];
            if letters {
                let letter = item.letter.map_or(String::new(), |c| c.to_string());
                let pad = 18usize.saturating_sub(item.label.chars().count());
                spans.push(Span::styled(format!("{}{letter}", " ".repeat(pad)), chrome.dim()));
            }
            if let (false, Some(note)) = (item.enabled, item.note) {
                spans.push(Span::styled(format!("  {note}"), chrome.dim()));
            }
            Line::from(spans)
        })
        .collect()
}

fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

fn draw_home(frame: &mut Frame, area: Rect, menu: &Menu<super::HomeItem>, chrome: &Chrome) {
    let mut lines = vec![
        Line::from(Span::styled("Distressing Damsel", chrome.title())),
        Line::from(Span::styled("sliding-block puzzles", chrome.dim())),
        Line::default(),
    ];
    lines.extend(menu_lines(menu, chrome, true));
    let height = lines.len() as u16;
    frame.render_widget(Paragraph::new(lines).style(chrome.text()), centred(area, 30, height));
}

fn draw_open(frame: &mut Frame, area: Rect, menu: &Menu<usize>, entries: &[Entry], chrome: &Chrome) {
    let mut lines = vec![Line::from(Span::styled("Open a puzzle", chrome.title())), Line::default()];
    let mut items = menu_lines(menu, chrome, false);
    for (line, entry) in items.iter_mut().zip(entries) {
        let tag = match entry {
            Entry::BuiltIn { .. } => "built-in".to_string(),
            Entry::File { file, .. } => file.clone(),
            Entry::Broken { error, .. } => format!("can't read: {error}"),
        };
        let used: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        line.spans.push(Span::styled(format!("{}{tag}", " ".repeat(30usize.saturating_sub(used))), chrome.dim()));
    }
    lines.extend(items);
    let width = area.width.min(72);
    let rect = Rect::new(area.x + (area.width - width) / 2, area.y + 2, width, area.height.saturating_sub(2));
    frame.render_widget(Paragraph::new(lines).style(chrome.text()), rect);
}

fn draw_placeholder(frame: &mut Frame, area: Rect, title: &str, note: &str, chrome: &Chrome) {
    let lines = vec![
        Line::from(Span::styled(title.to_string(), chrome.title())),
        Line::default(),
        Line::from(Span::styled(note.to_string(), chrome.dim())),
    ];
    let width = (note.chars().count() as u16).max(20);
    frame.render_widget(Paragraph::new(lines).style(chrome.text()), centred(area, width, 3));
}

/// A bordered box in the middle of `area`, over whatever is there.
fn draw_box(frame: &mut Frame, area: Rect, title: &str, lines: Vec<Line>, chrome: &Chrome) {
    draw_box_placed(frame, area, title, lines, chrome, false);
}

/// A bordered box over whatever is there: in the middle of `area`, or with
/// `at_right`, against its right-hand side so what is on the left (the
/// board) stays in view.
fn draw_box_placed(frame: &mut Frame, area: Rect, title: &str, lines: Vec<Line>, chrome: &Chrome, at_right: bool) {
    let width = lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16 + 6;
    let height = lines.len() as u16 + 2;
    let mut rect = centred(area, width.max(title.chars().count() as u16 + 4), height);
    if at_right {
        rect.x = area.x + area.width.saturating_sub(rect.width + 1);
    }
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(Span::styled(title.to_string(), chrome.title()))
        .border_style(chrome.dim())
        .style(chrome.text());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let inner = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), inner.height);
    frame.render_widget(Paragraph::new(lines).style(chrome.text()), inner);
}

/// Fades every cell in `area` outside `keep` towards the background.
fn dim_outside(frame: &mut Frame, area: Rect, keep: Rect, chrome: &Chrome) {
    use ratatui::style::Color;
    let fade = |c: Color| match c {
        Color::Rgb(r, g, b) => {
            let m = crate::look::Rgb(r, g, b).mix(chrome.background, 0.65);
            Color::Rgb(m.0, m.1, m.2)
        }
        other => other,
    };
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if keep.contains(ratatui::layout::Position { x, y }) {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, y)) {
                let (fg, bg) = (fade(cell.fg), fade(cell.bg));
                cell.set_fg(fg).set_bg(bg);
            }
        }
    }
}

/// The picture view's bar: the format, its keys, and for ANSI a note on
/// how best to look at the file.
fn draw_picture_bar(frame: &mut Frame, area: Rect, png: bool, settings: &crate::settings::Settings, chrome: &Chrome) {
    let (format, other) = if png { ("PNG picture", "ANSI") } else { ("ANSI picture", "PNG") };
    let mut spans = vec![Span::styled(format, chrome.selected())];
    for (key, what) in [("enter", "save".to_string()), ("a", other.to_string()), ("j", "labels on/off".into()), ("esc", "back".into())] {
        spans.push(Span::styled(" · ", chrome.dim()));
        spans.push(Span::styled(key, chrome.text()));
        spans.push(Span::styled(format!(" {what}"), chrome.dim()));
    }
    if !png && settings.picture_format != crate::settings::PictureFormat::Ansi {
        spans.push(Span::styled(" · cat the file in a terminal for best results", chrome.dim()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(chrome.text()), area);
}

/// The summary after a solve: how it went, and what can be done next.
fn draw_summary(frame: &mut Frame, area: Rect, summary: &super::Summary, shortest: Option<u32>, chrome: &Chrome) {
    let score = summary.score;
    let row = |name: &str, value: String| {
        Line::from(vec![Span::styled(format!("{name:<12}"), chrome.dim()), Span::styled(value, chrome.text())])
    };
    let mut lines = vec![
        row("moves", score.moves.to_string()),
        row("shortest", shortest.map_or("—".into(), |n| n.to_string())),
        row("hints", score.hints.to_string()),
    ];
    if score.solution_viewed {
        lines.push(row("solution", "viewed".into()));
    }
    if summary.new_best {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled("New best!", chrome.selected())));
    }
    lines.push(Line::default());
    let key = |k: &str, what: &str, live: bool| {
        Line::from(vec![
            Span::styled(format!("{k:<4}"), if live { chrome.selected() } else { chrome.dim() }),
            Span::styled(what.to_string(), if live { chrome.text() } else { chrome.dim() }),
        ])
    };
    lines.push(key("v", "look at the board", true));
    lines.push(key("p", "replay your solution", true));
    lines.push(key("s", "view the solution (not yet)", false));
    lines.push(key("o", "open another puzzle", true));
    draw_box(frame, area, " Solved! ", lines, chrome);
}

/// The preferences list: each setting and its value, the chosen row's
/// value between arrows.
fn draw_preferences(frame: &mut Frame, area: Rect, row: usize, settings: &crate::settings::Settings, chrome: &Chrome) {
    use super::prefs;
    let rows = prefs::rows(settings);
    let row = row.min(rows.len() - 1);
    let lines = rows
        .iter()
        .enumerate()
        .map(|(i, &pref)| {
            let chosen = i == row;
            let value = prefs::value(settings, pref);
            let style = if chosen { chrome.selected() } else { chrome.text() };
            Line::from(vec![
                Span::styled(if chosen { "▸ " } else { "  " }, style),
                Span::styled(format!("{:<16}", prefs::name(pref)), style),
                Span::styled(if chosen { format!("◂ {value} ▸") } else { format!("  {value}") }, style),
            ])
        })
        .collect();
    draw_box_placed(frame, area, " Preferences ", lines, chrome, true);
}

/// A yes-or-no dialogue: the question, the "don't ask again" box where it
/// is offered, and the two buttons, the focused one marked.
fn draw_confirm(frame: &mut Frame, area: Rect, confirm: &super::Confirm, chrome: &Chrome) {
    let button = |label: &str, focused: bool| {
        if focused {
            Span::styled(format!("▸ {label} "), chrome.selected())
        } else {
            Span::styled(format!("  {label} "), chrome.text())
        }
    };
    let mut lines = vec![Line::from(Span::styled(confirm.text, chrome.text())), Line::default()];
    if let Some(ticked) = confirm.dont_ask {
        let mark = if ticked { "[x]" } else { "[ ]" };
        lines.push(Line::from(vec![
            Span::styled(format!("{mark} "), chrome.text()),
            Span::styled("don't ask again", chrome.dim()),
            Span::styled("  (space)", chrome.dim()),
        ]));
        lines.push(Line::default());
    }
    lines.push(Line::from(vec![
        button("Yes (y)", confirm.yes_focused),
        Span::styled("   ", chrome.text()),
        button("No (n)", !confirm.yes_focused),
    ]));
    draw_box(frame, area, confirm.title, lines, chrome);
}

/// Keys of the context under the help box, from the same table that
/// handles them.
fn draw_help(frame: &mut Frame, area: Rect, context: Context, chrome: &Chrome) {
    let lines: Vec<Line> = keys::table(context)
        .iter()
        .filter(|b| !b.shown.is_empty())
        .map(|b| Line::from(vec![Span::styled(format!("{:<8}", b.shown), chrome.selected()), Span::styled(b.help, chrome.text())]))
        .collect();
    draw_box(frame, area, " Keys ", lines, chrome);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, chrome: &Chrome) {
    let line = match &app.message {
        Some(message) => Line::from(Span::styled(message.clone(), chrome.warn())),
        None => {
            // Whole hints only: those that do not fit are left off.
            let mut spans = Vec::new();
            let mut used = 0usize;
            for binding in keys::table(app.context()).iter().filter(|b| b.footer) {
                let sep = if spans.is_empty() { 0 } else { 3 };
                let width = sep + binding.shown.chars().count() + 1 + binding.help.chars().count();
                if used + width > area.width as usize {
                    break;
                }
                if sep > 0 {
                    spans.push(Span::styled(" · ", chrome.dim()));
                }
                spans.push(Span::styled(binding.shown, chrome.text()));
                spans.push(Span::styled(format!(" {}", binding.help), chrome.dim()));
                used += width;
            }
            Line::from(spans)
        }
    };
    frame.render_widget(Paragraph::new(line).style(chrome.text()), area);
}

#[cfg(test)]
mod tests {
    use crate::app::keys::Key;
    use crate::app::App;
    use crate::data::home::testing::TempDir;
    use crate::data::Home;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn screen_text(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut text = String::new();
        for y in 0..height {
            for x in 0..width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn home_open_game_and_overlays_render() {
        let tmp = TempDir::new();
        let mut app = App::new(Home::at(&tmp.0)).unwrap();
        let home = screen_text(&app, 80, 30);
        assert!(home.contains("Distressing Damsel") && home.contains("Open a puzzle"), "{home}");
        assert!(home.contains("enter open"));

        app.handle(Key::Char('o'));
        let open = screen_text(&app, 80, 30);
        assert!(open.contains("Klotski") && open.contains("built-in"), "{open}");

        app.handle(Key::Enter);
        let game = screen_text(&app, 80, 30);
        assert!(game.contains("First Steps") && game.contains("moves") && game.contains("? keys"), "{game}");

        app.handle(Key::Char('?'));
        let help = screen_text(&app, 80, 30);
        assert!(help.contains("Keys") && help.contains("pause menu"), "{help}");

        app.handle(Key::Esc);
        app.handle(Key::Char('.'));
        let pause = screen_text(&app, 80, 30);
        assert!(pause.contains("Paused") && pause.contains("Quit to home") && pause.contains("not yet"), "{pause}");
    }

    #[test]
    fn footer_hints_that_do_not_fit_are_left_off_whole() {
        let tmp = TempDir::new();
        let mut app = App::new(Home::at(&tmp.0)).unwrap();
        app.handle(Key::Char('o'));
        app.handle(Key::Enter);
        let narrow = screen_text(&app, 60, 30);
        let footer = narrow.lines().last().unwrap();
        assert!(footer.contains("s select a block"), "{footer}");
        assert!(!footer.contains("pause men") || footer.contains("pause menu"), "{footer}");
        assert!(footer.trim_end().ends_with("undo") || footer.trim_end().ends_with("keys"), "{footer}");
    }

    #[test]
    fn tiny_windows_do_not_panic() {
        let tmp = TempDir::new();
        let mut app = App::new(Home::at(&tmp.0)).unwrap();
        for (w, h) in [(1, 1), (5, 3), (20, 8)] {
            screen_text(&app, w, h);
        }
        app.handle(Key::Char('o'));
        app.handle(Key::Enter);
        app.handle(Key::Char('.'));
        for (w, h) in [(1, 1), (5, 3), (20, 8), (40, 12)] {
            screen_text(&app, w, h);
        }
    }
}
