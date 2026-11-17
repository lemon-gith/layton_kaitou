//! Position codes.
//!
//! A position is encoded by scanning the board in reading order and, at each
//! cell not already covered by a wall or an earlier block, writing one
//! symbol: empty, or the class anchored there. The number of symbols is the
//! same for every position of a layout, so codes have a fixed length.
//!
//! Two forms share that symbol stream: packed bytes, used as the solver's
//! key, and a short text form for people to copy around.

use std::fmt;

use crate::bits::{base64_decode, base64_encode, BitReader, BitWriter};
use crate::geometry::{bit, Cell, Mask};
use crate::layout::{ClassId, Layout};
use crate::position::Position;

const ALPHABET: &[u8; 62] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeError {
    /// Too short, too long, or contains a character that is not a symbol.
    Malformed,
    /// Decodes to something that is not a legal position of this layout.
    Illegal,
}

impl fmt::Display for CodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeError::Malformed => write!(f, "position code is malformed"),
            CodeError::Illegal => write!(f, "position code does not fit this puzzle"),
        }
    }
}

impl std::error::Error for CodeError {}

impl Layout {
    /// Length in bytes of every packed code for this layout.
    pub fn code_len(&self) -> usize {
        (self.n_symbols * self.symbol_bits as usize).div_ceil(8).max(1)
    }

    fn symbols(&self, position: &Position, mut emit: impl FnMut(u32)) {
        let mut covered: Mask = !self.open();
        for cell in 0..self.n_cells() as Cell {
            if covered & bit(cell) != 0 {
                continue;
            }
            match position.class_at(cell) {
                Some(class) => {
                    emit(class as u32 + 1);
                    covered |= self.block_mask(class, cell);
                }
                None => {
                    emit(0);
                    covered |= bit(cell);
                }
            }
        }
    }

    fn read_symbols(&self, mut next: impl FnMut() -> Option<u32>) -> Result<Position, CodeError> {
        let mut position = Position::EMPTY;
        let mut covered: Mask = !self.open();
        for cell in 0..self.n_cells() as Cell {
            if covered & bit(cell) != 0 {
                continue;
            }
            let symbol = next().ok_or(CodeError::Malformed)?;
            if symbol == 0 {
                covered |= bit(cell);
                continue;
            }
            let class = (symbol - 1) as usize;
            let mask = self.classes().get(class).map_or(0, |c| c.placement(cell));
            if mask == 0 || mask & covered != 0 {
                return Err(CodeError::Illegal);
            }
            position.place(cell, class as ClassId);
            covered |= mask;
        }
        self.check_position(&position).map_err(|_| CodeError::Illegal)?;
        Ok(position)
    }

    /// Writes the packed code of `position` into `out`, replacing its contents.
    pub fn encode_into(&self, position: &Position, out: &mut Vec<u8>) {
        out.clear();
        let bits = self.symbol_bits;
        let mut writer = BitWriter::new(out);
        self.symbols(position, |s| writer.write(s, bits));
        writer.finish();
        if out.is_empty() {
            out.push(0);
        }
    }

    pub fn encode(&self, position: &Position) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.code_len());
        self.encode_into(position, &mut out);
        out
    }

    /// Writes the smallest code among `position` and its mirror images, so
    /// that symmetric positions share one key. `scratch` is working space.
    pub fn encode_canonical_into(&self, position: &Position, out: &mut Vec<u8>, scratch: &mut Vec<u8>) {
        self.encode_into(position, out);
        for symmetry in self.symmetries() {
            self.encode_into(&self.transform(position, symmetry), scratch);
            if *scratch < *out {
                std::mem::swap(out, scratch);
            }
        }
    }

    pub fn decode(&self, code: &[u8]) -> Result<Position, CodeError> {
        if code.len() != self.code_len() {
            return Err(CodeError::Malformed);
        }
        let mut reader = BitReader::new(code);
        self.read_symbols(|| reader.read(self.symbol_bits))
    }

    /// Text form of a position: one character per symbol, `.` for empty and a
    /// letter or digit per class. Layouts with more than 62 classes fall back
    /// to `~` followed by the packed code in base64.
    pub fn position_string(&self, position: &Position) -> String {
        if self.classes().len() > ALPHABET.len() {
            return format!("~{}", base64_encode(&self.encode(position)));
        }
        let mut out = String::with_capacity(self.n_symbols);
        self.symbols(position, |s| {
            out.push(match s {
                0 => '.',
                s => ALPHABET[s as usize - 1] as char,
            })
        });
        out
    }

    pub fn parse_position_string(&self, text: &str) -> Result<Position, CodeError> {
        let text = text.trim();
        if let Some(packed) = text.strip_prefix('~') {
            return self.decode(&base64_decode(packed).ok_or(CodeError::Malformed)?);
        }
        let mut symbols = Vec::with_capacity(text.len());
        for byte in text.bytes() {
            symbols.push(match byte {
                b'.' => 0,
                b => ALPHABET.iter().position(|&a| a == b).ok_or(CodeError::Malformed)? as u32 + 1,
            });
        }
        if symbols.len() != self.n_symbols {
            return Err(CodeError::Malformed);
        }
        let mut symbols = symbols.into_iter();
        self.read_symbols(|| symbols.next())
    }
}
