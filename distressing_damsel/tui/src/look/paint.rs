//! Paints a game onto a [`Canvas`].

use std::collections::HashMap;

use dd_engine::{bit, Game, Mask};

use super::theme::{grain, Rgb, Theme};

/// One terminal character.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
}

/// A rectangle of styled characters, ready for any terminal backend.
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub glyphs: Vec<Glyph>,
}

impl Canvas {
    /// The canvas as 24-bit ANSI text, one line per row.
    pub fn to_ansi(&self) -> String {
        let mut out = String::new();
        for row in self.glyphs.chunks(self.width) {
            let mut last: Option<(Rgb, Rgb, bool)> = None;
            for g in row {
                if last != Some((g.fg, g.bg, g.bold)) {
                    out.push_str(&format!(
                        "\x1b[0;{}38;2;{};{};{};48;2;{};{};{}m",
                        if g.bold { "1;" } else { "" },
                        g.fg.0,
                        g.fg.1,
                        g.fg.2,
                        g.bg.0,
                        g.bg.1,
                        g.bg.2
                    ));
                    last = Some((g.fg, g.bg, g.bold));
                }
                out.push(g.ch);
            }
            out.push_str("\x1b[0m\n");
        }
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct PaintOptions {
    /// Characters per cell across; cells are half as many rows tall, which
    /// is square in most terminals. Even, at least 4.
    pub scale: usize,
    /// Text to write on each block, by block id.
    pub labels: HashMap<usize, String>,
    /// Block to outline as selected.
    pub selected: Option<usize>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Owner {
    Frame,
    Floor,
    Wall,
    Face(usize),
}

const FRAME_SIDE: usize = 4;
const FRAME_TOP: usize = 3;
const FRAME_BOTTOM: usize = 4;

pub fn paint(game: &Game, theme: &Theme, options: &PaintOptions) -> Canvas {
    let layout = game.layout();
    let s = options.scale.max(4) & !1;
    let (w, h) = (layout.width() as usize, layout.height() as usize);
    // One pixel of floor before the first cell matches the gap every cell
    // leaves after itself.
    let (left, top) = (FRAME_SIDE + 1, FRAME_TOP + 1);
    let pw = left + w * s + FRAME_SIDE;
    let ph = top + h * s + FRAME_BOTTOM;

    let block_of: Vec<Option<usize>> = (0..layout.n_cells()).map(|c| game.block_at(c as u8)).collect();
    let cell_kind = |cx: isize, cy: isize| -> Option<Result<usize, ()>> {
        if cx < 0 || cy < 0 || cx >= w as isize || cy >= h as isize {
            return None;
        }
        let cell = layout.cell(cx as u8, cy as u8);
        if layout.is_wall(cell) {
            Some(Err(()))
        } else {
            block_of[cell as usize].map(Ok)
        }
    };

    // Who owns each pixel.
    let mut owner = vec![Owner::Floor; pw * ph];
    for y in 0..ph {
        for x in 0..pw {
            let o = &mut owner[y * pw + x];
            if x < FRAME_SIDE || x >= pw - FRAME_SIDE || y < FRAME_TOP || y >= ph - FRAME_BOTTOM {
                *o = Owner::Frame;
                continue;
            }
            let bx = x as isize - left as isize;
            let by = y as isize - top as isize;
            let (cx, cy) = (bx.max(0) / s as isize, by.max(0) / s as isize);
            let (cx, cy) = (cx.min(w as isize - 1), cy.min(h as isize - 1));
            let kind = cell_kind(cx, cy);
            if kind == Some(Err(())) {
                *o = Owner::Wall;
                continue;
            }
            if bx < 0 || by < 0 {
                continue;
            }
            let Some(Ok(block)) = kind else { continue };
            let (lx, ly) = (bx as usize % s, by as usize % s);
            let same = |dx: isize, dy: isize| cell_kind(cx + dx, cy + dy) == Some(Ok(block));
            let right_gap = lx == s - 1 && !same(1, 0);
            let down_gap = ly == s - 1 && !same(0, 1);
            let corner_gap = lx == s - 1 && ly == s - 1 && !same(1, 1);
            if !(right_gap || down_gap || corner_gap) {
                *o = Owner::Face(block);
            }
        }
    }
    let at = |x: isize, y: isize| -> Owner {
        if x < 0 || y < 0 || x >= pw as isize || y >= ph as isize {
            Owner::Frame
        } else {
            owner[y as usize * pw + x as usize]
        }
    };

    // Goal areas and where they meet the edge of the board.
    let mut goal_cells: Mask = 0;
    for (class, anchor) in layout.goal().targets() {
        goal_cells |= layout.block_mask(class, anchor);
    }
    let in_goal = |x: isize, y: isize| -> bool {
        let (bx, by) = (x - left as isize, y - top as isize);
        if bx < 0 || by < 0 || bx >= (w * s) as isize || by >= (h * s) as isize {
            return false;
        }
        let (cx, cy) = (bx as usize / s, by as usize / s);
        goal_cells & bit(layout.cell(cx as u8, cy as u8)) != 0
    };

    // The goal area inset the way a block's face is, so its outline sits
    // where the block's edge will.
    let goal_face = |x: isize, y: isize| -> bool {
        if !in_goal(x, y) {
            return false;
        }
        let (lx, ly) = ((x - left as isize) as usize % s, (y - top as isize) as usize % s);
        !(lx == s - 1 && !in_goal(x + 1, y)) && !(ly == s - 1 && !in_goal(x, y + 1))
    };

    let key_classes: Vec<u8> = layout.goal().targets().iter().map(|t| t.0).collect();
    let colour_of_block = |block: usize| -> Rgb {
        let class = game.puzzle().blocks[block].class;
        if key_classes.contains(&class) && key_classes.len() == 1 {
            return theme.key;
        }
        let others = (0..class).filter(|c| !key_classes.contains(c) || key_classes.len() > 1).count();
        theme.blocks[others % theme.blocks.len()]
    };

    let raised = |o: Owner| matches!(o, Owner::Frame | Owner::Wall | Owner::Face(_));
    let mut pixels = vec![Rgb(0, 0, 0); pw * ph];
    for y in 0..ph {
        for x in 0..pw {
            let (xi, yi) = (x as isize, y as isize);
            let here = owner[y * pw + x];
            let g = theme.grain;
            let colour = match here {
                Owner::Frame => {
                    // Mitred frame: grain follows the nearest outer edge.
                    let d = [x, pw - 1 - x, y, ph - 1 - y];
                    let side = (0..4).min_by_key(|&i| d[i]).unwrap();
                    let (along, across, thick) = match side {
                        0 | 1 => (y as i32, d[side] as i32, FRAME_SIDE),
                        2 => (x as i32, d[side] as i32, FRAME_TOP),
                        _ => (x as i32, d[side] as i32, FRAME_BOTTOM),
                    };
                    let mut c = theme.frame.shade(grain(theme.frame_grain, along, across, side as u32 + 3) * 0.22 * g);
                    // Light from the top left: the outer edge catches it on
                    // the top and left, the inner edge on the bottom and right.
                    let lit_side = side == 0 || side == 2;
                    if d[side] == 0 {
                        c = c.shade(if lit_side { 0.6 } else { -0.6 } * theme.bevel);
                    } else if d[side] == thick - 1 {
                        c = c.shade(if lit_side { -0.9 } else { 0.5 } * theme.bevel);
                    }
                    c
                }
                Owner::Wall => {
                    let mut c = theme.wall.shade(grain(theme.wall_grain, x as i32, y as i32, 11) * 0.22 * g);
                    if theme.grain > 0.0 {
                        if !matches!(at(xi, yi - 1), Owner::Wall | Owner::Frame)
                            || !matches!(at(xi - 1, yi), Owner::Wall | Owner::Frame)
                        {
                            c = c.shade(theme.bevel * 0.6);
                        } else if !matches!(at(xi, yi + 1), Owner::Wall | Owner::Frame)
                            || !matches!(at(xi + 1, yi), Owner::Wall | Owner::Frame)
                        {
                            c = c.shade(-theme.bevel * 0.8);
                        }
                    }
                    c
                }
                Owner::Floor => {
                    let mut c = theme.floor.shade(grain(theme.floor_grain, x as i32, y as i32, 7) * 0.25 * g);
                    let (bx, by) = (x as isize - left as isize, y as isize - top as isize);
                    if bx < 0 || by < 0 || bx as usize % s == s - 1 || by as usize % s == s - 1 {
                        c = c.shade(if theme.dark { theme.floor_grid } else { -theme.floor_grid });
                    }
                    if goal_face(xi, yi) {
                        let edge = [(0, -1), (0, 1), (-1, 0), (1, 0)]
                            .iter()
                            .any(|&(dx, dy)| !goal_face(xi + dx, yi + dy));
                        c = c.mix(theme.goal, if edge { 0.7 } else { 0.12 });
                    }
                    if theme.shadow > 0.0 {
                        let cast = raised(at(xi, yi - 1)) as u8 + raised(at(xi - 1, yi)) as u8;
                        if cast > 0 {
                            c = c.shade(-theme.shadow * if cast == 2 { 1.2 } else { 1.0 });
                        } else if raised(at(xi - 1, yi - 1)) {
                            c = c.shade(-theme.shadow * 0.5);
                        }
                    }
                    c
                }
                Owner::Face(block) => {
                    let base = colour_of_block(block);
                    let seed = 101 + block as u32 * 7;
                    let mut c = base.shade(grain(theme.block_grain, x as i32, y as i32, seed) * 0.25 * g);
                    if theme.grain > 0.0 {
                        let lit = at(xi, yi - 1) != here || at(xi - 1, yi) != here;
                        let dim = at(xi, yi + 1) != here || at(xi + 1, yi) != here;
                        c = match (lit, dim) {
                            (true, false) => c.shade(theme.bevel),
                            (false, true) => c.shade(-theme.bevel * 1.3),
                            _ => c,
                        };
                    }
                    c
                }
            };
            pixels[y * pw + x] = colour;
        }
    }

    // The exit: where a lone target touches the edge, the frame takes its
    // colour. Puzzles with several targets have no single way out.
    let single_target = layout.goal().targets().len() == 1;
    for y in top..top + h * s {
        for x in left..left + w * s {
            if !single_target || !goal_face(x as isize, y as isize) {
                continue;
            }
            let mut mark = |px: usize, py: usize| {
                let i = py * pw + px;
                if owner[i] == Owner::Frame {
                    pixels[i] = pixels[i].mix(theme.goal, 0.8);
                }
            };
            let (right, bottom) = (left + w * s, top + h * s);
            if !in_goal(x as isize + 2, y as isize) && x + 2 >= right {
                (right + 1..pw - 1).for_each(|px| mark(px, y));
            }
            if x == left {
                (1..FRAME_SIDE).for_each(|px| mark(px, y));
            }
            if !in_goal(x as isize, y as isize + 2) && y + 2 >= bottom {
                (bottom + 1..ph - 1).for_each(|py| mark(x, py));
            }
            if y == top {
                (1..FRAME_TOP).for_each(|py| mark(x, py));
            }
        }
    }

    // Selection: a ring just outside the block.
    if let Some(sel) = options.selected {
        let face = |x: isize, y: isize| at(x, y) == Owner::Face(sel);
        let mut ring = Vec::new();
        for y in 0..ph as isize {
            for x in 0..pw as isize {
                if !face(x, y) && (-1..=1).any(|dy| (-1..=1).any(|dx| face(x + dx, y + dy))) {
                    ring.push(y as usize * pw + x as usize);
                }
            }
        }
        for i in ring {
            pixels[i] = theme.accent;
        }
    }

    // Two pixels per character.
    let (cw, ch) = (pw, ph / 2);
    let mut glyphs = Vec::with_capacity(cw * ch);
    for row in 0..ch {
        for x in 0..cw {
            let (upper, lower) = (pixels[row * 2 * pw + x], pixels[(row * 2 + 1) * pw + x]);
            glyphs.push(if upper == lower {
                Glyph { ch: ' ', fg: upper, bg: upper, bold: false }
            } else {
                Glyph { ch: '▀', fg: upper, bg: lower, bold: false }
            });
        }
    }

    // Labels go on the run of whole characters nearest the middle of the face.
    for (&block, text) in &options.labels {
        let len = text.chars().count();
        let face = |x: usize, y: usize| owner[y * pw + x] == Owner::Face(block);
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
        for y in 0..ph {
            for x in 0..pw {
                if face(x, y) {
                    sx += x as f32;
                    sy += y as f32;
                    n += 1.0;
                }
            }
        }
        if n == 0.0 {
            continue;
        }
        let (mx, my) = (sx / n, sy / n);
        let mut best: Option<(f32, usize, usize)> = None;
        for row in 0..ch {
            for x in 0..cw.saturating_sub(len - 1) {
                let fits = (0..len).all(|i| face(x + i, row * 2) && face(x + i, row * 2 + 1));
                if !fits {
                    continue;
                }
                let dx = x as f32 + (len as f32 - 1.0) / 2.0 - mx;
                let dy = row as f32 * 2.0 + 0.5 - my;
                let d = dx * dx + dy * dy;
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, x, row));
                }
            }
        }
        if let Some((_, x, row)) = best {
            let face_colour = colour_of_block(block);
            let fg = if face_colour.luma() > 0.62 { theme.text_on_light } else { theme.text };
            for (i, c) in text.chars().enumerate() {
                let g = &mut glyphs[row * cw + x + i];
                let bg = pixels[row * 2 * pw + x + i].mix(pixels[(row * 2 + 1) * pw + x + i], 0.5);
                *g = Glyph { ch: c, fg, bg, bold: true };
            }
        }
    }

    Canvas { width: cw, height: ch, glyphs }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::THEMES;

    #[test]
    fn every_puzzle_paints_in_every_theme() {
        for (_, source) in crate::BUILT_IN {
            let (puzzle, _) = dd_engine::file::parse(source).unwrap().build().unwrap();
            let game = Game::new(puzzle);
            let labels: HashMap<usize, String> =
                (0..game.block_count()).map(|b| (b, format!("{b}"))).collect();
            for theme in &THEMES {
                for scale in [4, 6, 8, 12] {
                    let options = PaintOptions { scale, labels: labels.clone(), selected: Some(0) };
                    let canvas = paint(&game, theme, &options);
                    assert_eq!(canvas.glyphs.len(), canvas.width * canvas.height);
                    let layout = game.layout();
                    assert_eq!(canvas.width, layout.width() as usize * scale + 2 * FRAME_SIDE + 1);
                    // Every block gets its label somewhere.
                    for b in 0..game.block_count() {
                        let text = b.to_string();
                        let rows: Vec<String> = canvas
                            .glyphs
                            .chunks(canvas.width)
                            .map(|r| r.iter().map(|g| if g.bold { g.ch } else { ' ' }).collect())
                            .collect();
                        assert!(rows.iter().any(|r| r.contains(&text)), "label {text} missing");
                    }
                }
            }
        }
    }
}
