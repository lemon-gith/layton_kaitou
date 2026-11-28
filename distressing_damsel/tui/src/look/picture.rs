//! Saving pictures of the board as PNG files.
//!
//! The painter's pixels are enlarged into square blocks of colour, and
//! block labels are drawn on top with a small built-in pixel font, so the
//! picture matches what is on screen.

use super::paint::Picture;
use super::Rgb;

/// Each painted pixel becomes this many image pixels across and down. A
/// character on screen is one pixel wide and two tall, so a label's
/// character gets a 12 by 24 space.
pub const ENLARGE: usize = 12;

/// Glyphs drawn at twice their 5 by 7 size: 10 by 14 image pixels.
const GLYPH_SCALE: usize = 2;

/// A 5 by 7 font for block labels: digits, then capitals, then lower case.
/// Each row is five bits, the leftmost pixel highest.
const FONT: [(char, [u8; 7]); 62] = [
    ('0', [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
    ('1', [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('2', [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111]),
    ('3', [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110]),
    ('4', [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
    ('5', [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
    ('6', [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
    ('7', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
    ('8', [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
    ('9', [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
    ('A', [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('B', [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
    ('C', [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
    ('D', [0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100]),
    ('E', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111]),
    ('F', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('G', [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111]),
    ('H', [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('I', [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('J', [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
    ('K', [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001]),
    ('L', [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
    ('M', [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001]),
    ('N', [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001]),
    ('O', [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('P', [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('Q', [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101]),
    ('R', [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
    ('S', [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
    ('T', [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('U', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('V', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
    ('W', [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010]),
    ('X', [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001]),
    ('Y', [0b10001, 0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100]),
    ('Z', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
    ('a', [0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111]),
    ('b', [0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b11110]),
    ('c', [0b00000, 0b00000, 0b01110, 0b10000, 0b10000, 0b10001, 0b01110]),
    ('d', [0b00001, 0b00001, 0b01101, 0b10011, 0b10001, 0b10001, 0b01111]),
    ('e', [0b00000, 0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110]),
    ('f', [0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000]),
    ('g', [0b00000, 0b01111, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110]),
    ('h', [0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001]),
    ('i', [0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('j', [0b00010, 0b00000, 0b00110, 0b00010, 0b00010, 0b10010, 0b01100]),
    ('k', [0b10000, 0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010]),
    ('l', [0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('m', [0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10001, 0b10001]),
    ('n', [0b00000, 0b00000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001]),
    ('o', [0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('p', [0b00000, 0b00000, 0b11110, 0b10001, 0b11110, 0b10000, 0b10000]),
    ('q', [0b00000, 0b00000, 0b01101, 0b10011, 0b01111, 0b00001, 0b00001]),
    ('r', [0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000]),
    ('s', [0b00000, 0b00000, 0b01110, 0b10000, 0b01110, 0b00001, 0b11110]),
    ('t', [0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110]),
    ('u', [0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101]),
    ('v', [0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
    ('w', [0b00000, 0b00000, 0b10001, 0b10001, 0b10101, 0b10101, 0b01010]),
    ('x', [0b00000, 0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001]),
    ('y', [0b00000, 0b00000, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110]),
    ('z', [0b00000, 0b00000, 0b11111, 0b00010, 0b00100, 0b01000, 0b11111]),
];

fn glyph(c: char) -> Option<&'static [u8; 7]> {
    FONT.iter().find(|(g, _)| *g == c).map(|(_, rows)| rows)
}

/// The picture enlarged, with its labels, as rows of RGB bytes.
pub fn rgb_image(picture: &Picture) -> (usize, usize, Vec<u8>) {
    let (w, h) = (picture.width * ENLARGE, picture.height * ENLARGE);
    let mut data = vec![0u8; w * h * 3];
    let mut put = |x: usize, y: usize, c: Rgb| {
        if x < w && y < h {
            let i = (y * w + x) * 3;
            data[i..i + 3].copy_from_slice(&[c.0, c.1, c.2]);
        }
    };
    for y in 0..h {
        for x in 0..w {
            put(x, y, picture.pixels[(y / ENLARGE) * picture.width + x / ENLARGE]);
        }
    }
    // Centre each glyph in its character's space.
    let (gw, gh) = (5 * GLYPH_SCALE, 7 * GLYPH_SCALE);
    let (pad_x, pad_y) = ((ENLARGE - gw) / 2, (2 * ENLARGE - gh) / 2);
    for label in &picture.labels {
        for (i, c) in label.text.chars().enumerate() {
            let Some(rows) = glyph(c) else { continue };
            let x0 = (label.x + i) * ENLARGE + pad_x;
            let y0 = label.row * 2 * ENLARGE + pad_y;
            for (r, bits) in rows.iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        for dy in 0..GLYPH_SCALE {
                            for dx in 0..GLYPH_SCALE {
                                put(x0 + col * GLYPH_SCALE + dx, y0 + r * GLYPH_SCALE + dy, label.fg);
                            }
                        }
                    }
                }
            }
        }
    }
    (w, h, data)
}

/// The picture as a PNG file.
pub fn png(picture: &Picture) -> Result<Vec<u8>, String> {
    let (w, h, data) = rgb_image(picture);
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&data).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::paint::PictureLabel;

    #[test]
    fn every_label_character_has_a_glyph() {
        for c in ('0'..='9').chain('A'..='Z').chain('a'..='z') {
            assert!(glyph(c).is_some(), "{c}");
        }
        assert!(glyph('?').is_none());
    }

    #[test]
    fn pixels_are_enlarged_and_labels_drawn_on_top() {
        let (bg, fg) = (Rgb(10, 20, 30), Rgb(250, 250, 250));
        let picture = Picture {
            width: 2,
            height: 2,
            pixels: vec![bg, Rgb(1, 2, 3), bg, bg],
            labels: vec![PictureLabel { x: 0, row: 0, text: "I".into(), fg }],
        };
        let (w, h, data) = rgb_image(&picture);
        assert_eq!((w, h), (24, 24));
        let at = |x: usize, y: usize| Rgb(data[(y * w + x) * 3], data[(y * w + x) * 3 + 1], data[(y * w + x) * 3 + 2]);
        // The top-right painted pixel fills its 12 by 12 square.
        assert_eq!(at(12, 0), Rgb(1, 2, 3));
        assert_eq!(at(23, 11), Rgb(1, 2, 3));
        // "I" has a full-width top bar in its second to fourth columns.
        assert_eq!(at(1 + 2 * 2, 5), fg);
        assert_eq!(at(0, 0), bg);

        let file = png(&picture).unwrap();
        assert_eq!(&file[1..4], b"PNG");
    }
}
