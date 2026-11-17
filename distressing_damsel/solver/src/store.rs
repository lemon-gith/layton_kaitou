//! Append-only set of fixed-length keys, numbered in insertion order.
//!
//! Keys live back to back in one buffer and an open-addressing table maps a
//! key to its number, so a state costs its key plus about twelve bytes.

const EMPTY: u32 = u32::MAX;

pub(crate) struct Store {
    key_len: usize,
    keys: Vec<u8>,
    parents: Vec<u32>,
    table: Vec<u32>,
}

impl Store {
    pub fn new(key_len: usize) -> Store {
        Store { key_len, keys: Vec::new(), parents: Vec::new(), table: vec![EMPTY; 1024] }
    }

    pub fn len(&self) -> usize {
        self.parents.len()
    }

    pub fn key(&self, id: u32) -> &[u8] {
        let start = id as usize * self.key_len;
        &self.keys[start..start + self.key_len]
    }

    pub fn parent(&self, id: u32) -> u32 {
        self.parents[id as usize]
    }

    fn hash(key: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for chunk in key.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            h = (h ^ u64::from_le_bytes(word)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            h ^= h >> 29;
        }
        h
    }

    /// Slot holding `key`, or the empty slot where it belongs.
    fn slot(&self, key: &[u8]) -> usize {
        let mask = self.table.len() - 1;
        let mut slot = Self::hash(key) as usize & mask;
        loop {
            match self.table[slot] {
                EMPTY => return slot,
                id if self.key(id) == key => return slot,
                _ => slot = (slot + 1) & mask,
            }
        }
    }

    pub fn find(&self, key: &[u8]) -> Option<u32> {
        match self.table[self.slot(key)] {
            EMPTY => None,
            id => Some(id),
        }
    }

    /// Returns the key's number and whether it was new. `parent` is recorded
    /// only for new keys.
    pub fn insert(&mut self, key: &[u8], parent: u32) -> (u32, bool) {
        debug_assert_eq!(key.len(), self.key_len);
        let slot = self.slot(key);
        if self.table[slot] != EMPTY {
            return (self.table[slot], false);
        }
        let id = self.parents.len() as u32;
        self.keys.extend_from_slice(key);
        self.parents.push(parent);
        self.table[slot] = id;
        if self.parents.len() * 2 > self.table.len() {
            self.grow();
        }
        (id, true)
    }

    fn grow(&mut self) {
        self.table = vec![EMPTY; self.table.len() * 2];
        for id in 0..self.parents.len() as u32 {
            let slot = self.slot(self.key(id));
            self.table[slot] = id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_and_finds_across_growth() {
        let mut store = Store::new(3);
        for i in 0..5000u32 {
            let key = [(i >> 16) as u8, (i >> 8) as u8, i as u8];
            assert_eq!(store.insert(&key, i), (i, true));
        }
        for i in 0..5000u32 {
            let key = [(i >> 16) as u8, (i >> 8) as u8, i as u8];
            assert_eq!(store.insert(&key, 0), (i, false));
            assert_eq!(store.find(&key), Some(i));
            assert_eq!(store.parent(i), i);
        }
        assert_eq!(store.find(&[9, 9, 9]), None);
        assert_eq!(store.len(), 5000);
    }
}
