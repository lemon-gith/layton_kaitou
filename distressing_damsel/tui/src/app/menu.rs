//! A vertical menu of items, each with a shortcut letter.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<T> {
    pub label: String,
    pub letter: Option<char>,
    pub enabled: bool,
    pub value: T,
}

#[derive(Clone, Debug)]
pub struct Menu<T> {
    pub items: Vec<Item<T>>,
    pub selected: usize,
}

impl<T: Copy> Menu<T> {
    pub fn new(items: Vec<Item<T>>) -> Menu<T> {
        let mut menu = Menu { items, selected: 0 };
        if !menu.items.is_empty() && !menu.items[0].enabled {
            menu.step(1);
        }
        menu
    }

    /// Moves to the next enabled item, wrapping round.
    pub fn step(&mut self, by: isize) {
        let n = self.items.len() as isize;
        if n == 0 {
            return;
        }
        for i in 1..=n {
            let next = (self.selected as isize + by * i).rem_euclid(n) as usize;
            if self.items[next].enabled {
                self.selected = next;
                return;
            }
        }
    }

    pub fn chosen(&self) -> Option<T> {
        self.items.get(self.selected).filter(|i| i.enabled).map(|i| i.value)
    }

    /// The enabled item with this shortcut letter.
    pub fn by_letter(&self, letter: char) -> Option<T> {
        self.items.iter().find(|i| i.enabled && i.letter == Some(letter)).map(|i| i.value)
    }
}

pub fn item<T>(label: impl Into<String>, letter: Option<char>, enabled: bool, value: T) -> Item<T> {
    Item { label: label.into(), letter, enabled, value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_disabled_items_and_wraps() {
        let mut m = Menu::new(vec![
            item("a", Some('a'), false, 0),
            item("b", Some('b'), true, 1),
            item("c", Some('c'), false, 2),
            item("d", Some('d'), true, 3),
        ]);
        assert_eq!(m.chosen(), Some(1));
        m.step(1);
        assert_eq!(m.chosen(), Some(3));
        m.step(1);
        assert_eq!(m.chosen(), Some(1));
        m.step(-1);
        assert_eq!(m.chosen(), Some(3));
        assert_eq!(m.by_letter('c'), None);
        assert_eq!(m.by_letter('d'), Some(3));
    }
}
