//! Bit-level packing and URL-safe base64, shared by the code formats.

pub(crate) struct BitWriter<'a> {
    out: &'a mut Vec<u8>,
    acc: u32,
    pending: u32,
}

impl<'a> BitWriter<'a> {
    pub fn new(out: &'a mut Vec<u8>) -> Self {
        BitWriter { out, acc: 0, pending: 0 }
    }

    /// Appends the low `bits` bits of `value`, most significant first.
    /// `bits` must be at most 16.
    #[inline]
    pub fn write(&mut self, value: u32, bits: u32) {
        debug_assert!(bits <= 16 && (bits == 32 || value >> bits == 0));
        self.acc = (self.acc << bits) | value;
        self.pending += bits;
        while self.pending >= 8 {
            self.pending -= 8;
            self.out.push((self.acc >> self.pending) as u8);
        }
        self.acc &= (1 << self.pending) - 1;
    }

    pub fn finish(self) {
        if self.pending > 0 {
            self.out.push((self.acc << (8 - self.pending)) as u8);
        }
    }
}

pub(crate) struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0 }
    }

    #[inline]
    pub fn read(&mut self, bits: u32) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..bits {
            let byte = *self.data.get(self.pos >> 3)?;
            value = (value << 1) | ((byte >> (7 - (self.pos & 7))) & 1) as u32;
            self.pos += 1;
        }
        Some(value)
    }
}

/// Bits needed to store any value in `0..=max`.
pub(crate) fn bits_for(max: usize) -> u32 {
    (usize::BITS - max.leading_zeros()).max(1)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub(crate) fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let mut buf = [0u8; 3];
        buf[..chunk.len()].copy_from_slice(chunk);
        let n = u32::from_be_bytes([0, buf[0], buf[1], buf[2]]);
        for i in 0..=chunk.len() {
            out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

pub(crate) fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let digits: Option<Vec<u32>> = text
        .bytes()
        .map(|b| B64.iter().position(|&c| c == b).map(|p| p as u32))
        .collect();
    for chunk in digits?.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut n = 0u32;
        for (i, d) in chunk.iter().enumerate() {
            n |= d << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_round_trip() {
        let mut buf = Vec::new();
        let mut w = BitWriter::new(&mut buf);
        let values = [(5, 3), (0, 1), (255, 8), (1, 1), (1000, 10), (3, 2)];
        for (v, b) in values {
            w.write(v, b);
        }
        w.finish();
        let mut r = BitReader::new(&buf);
        for (v, b) in values {
            assert_eq!(r.read(b), Some(v));
        }
    }

    #[test]
    fn base64_round_trip() {
        for len in 0..20u8 {
            let data: Vec<u8> = (0..len).map(|i| i.wrapping_mul(37) ^ 0xa5).collect();
            assert_eq!(base64_decode(&base64_encode(&data)), Some(data));
        }
        assert_eq!(base64_decode("a"), None);
        assert_eq!(base64_decode("a+b"), None);
    }

    #[test]
    fn bit_widths() {
        assert_eq!(bits_for(0), 1);
        assert_eq!(bits_for(1), 1);
        assert_eq!(bits_for(2), 2);
        assert_eq!(bits_for(3), 2);
        assert_eq!(bits_for(4), 3);
    }
}
