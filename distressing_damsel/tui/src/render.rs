//! Plain-text drawing of boards, for the headless commands.

use dd_engine::{bit, Game, Mask};

/// Draws the board, one row per line. Blocks show their label, walls `#`,
/// empty cells `.`, and empty goal cells `+`.
pub fn board(game: &Game) -> String {
    let layout = game.layout();
    let targets: Mask = layout
        .goal()
        .targets()
        .iter()
        .fold(0, |acc, &(class, anchor)| acc | layout.block_mask(class, anchor));
    let mut out = String::new();
    for y in 0..layout.height() {
        out.push_str("  ");
        for x in 0..layout.width() {
            let cell = layout.cell(x, y);
            let ch = match game.block_at(cell) {
                Some(block) => game.puzzle().blocks[block].label,
                None if layout.is_wall(cell) => '#',
                None if targets & bit(cell) != 0 => '+',
                None => '.',
            };
            out.push(ch);
            out.push(' ');
        }
        out.push('\n');
    }
    out
}

/// A row of bars, one per value, scaled to the largest.
pub fn sparkline(values: &[usize]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = values.iter().copied().max().unwrap_or(0).max(1);
    values.iter().map(|&v| BARS[(v * 7).div_ceil(max).min(7)]).collect()
}
