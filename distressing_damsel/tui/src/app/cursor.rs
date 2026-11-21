//! Moving the selection cursor between blocks with the arrow keys.

use dd_engine::{cells, BlockId, Dir, Game};

/// The block the cursor moves to from `from` when an arrow is pressed.
///
/// 1. Look along the arrow within the band of rows (or columns) the current
///    block covers, and take the nearest block with a cell in that band.
/// 2. If there is none, take the nearest block anywhere in that direction,
///    measured between centres, with sideways distance counting double so
///    blocks straight ahead win.
/// 3. If there is nothing in that direction, stay put (`None`).
pub fn neighbour(game: &Game, from: BlockId, dir: Dir) -> Option<BlockId> {
    let layout = game.layout();
    let xy = |b: BlockId| -> Vec<(i32, i32)> {
        cells(game.block_cells(b))
            .map(|c| {
                let (x, y) = layout.xy(c);
                (x as i32, y as i32)
            })
            .collect()
    };
    // Coordinates turned so the arrow points along +along.
    let turn = |(x, y): (i32, i32)| -> (i32, i32) {
        match dir {
            Dir::Right => (x, y),
            Dir::Left => (-x, y),
            Dir::Down => (y, x),
            Dir::Up => (-y, x),
        }
    };
    let mine: Vec<(i32, i32)> = xy(from).into_iter().map(turn).collect();
    let front = mine.iter().map(|c| c.0).max()?;
    let band: Vec<i32> = mine.iter().map(|c| c.1).collect();
    let centre = |cs: &[(i32, i32)]| {
        let n = cs.len() as f32;
        (cs.iter().map(|c| c.0 as f32).sum::<f32>() / n, cs.iter().map(|c| c.1 as f32).sum::<f32>() / n)
    };
    let (ma, mp) = centre(&mine);

    let others: Vec<(BlockId, Vec<(i32, i32)>)> = (0..game.block_count())
        .filter(|&b| b != from)
        .map(|b| (b, xy(b).into_iter().map(turn).collect()))
        .collect();

    // 1. Within the band.
    let in_band = others
        .iter()
        .filter_map(|(b, cs)| {
            let gap = cs.iter().filter(|c| c.0 > front && band.contains(&c.1)).map(|c| c.0 - front).min()?;
            let (_, p) = centre(cs);
            Some((gap, (p - mp).abs(), *b))
        })
        .min_by(|a, b| a.partial_cmp(b).expect("finite"));
    if let Some((_, _, b)) = in_band {
        return Some(b);
    }

    // 2. Anywhere ahead.
    others
        .iter()
        .filter_map(|(b, cs)| {
            let (a, p) = centre(cs);
            (a > ma).then(|| ((a - ma) + 2.0 * (p - mp).abs(), *b))
        })
        .min_by(|a, b| a.partial_cmp(b).expect("finite"))
        .map(|(_, b)| b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_engine::PuzzleSpec;

    fn game(grid: &str) -> Game {
        let mut spec = PuzzleSpec::from_grid(grid).unwrap();
        let key = spec.cells.iter().find_map(|c| match c {
            dd_engine::CellSpec::Block(l) => Some(*l),
            _ => None,
        });
        spec.goals.push(dd_engine::GoalSpec { block: key.unwrap(), x: 0, y: 0 });
        Game::new(spec.build().unwrap().0)
    }

    fn id(game: &Game, label: char) -> BlockId {
        game.puzzle().blocks.iter().position(|b| b.label == label).unwrap()
    }

    fn go(game: &Game, from: char, dir: Dir) -> Option<char> {
        neighbour(game, id(game, from), dir).map(|b| game.puzzle().blocks[b].label)
    }

    #[test]
    fn klotski_moves_between_neighbours() {
        let spec = dd_engine::file::parse(include_str!("../../../puzzles/klotski.toml")).unwrap();
        let g = Game::new(spec.build().unwrap().0);
        // ABBC / ABBC / DEEF / DGHF / I..J
        assert_eq!(go(&g, 'A', Dir::Right), Some('B'));
        assert_eq!(go(&g, 'B', Dir::Right), Some('C'));
        assert_eq!(go(&g, 'C', Dir::Right), None);
        assert_eq!(go(&g, 'B', Dir::Down), Some('E'));
        assert_eq!(go(&g, 'E', Dir::Down), Some('G'), "a tie goes to the lower id");
        assert_eq!(go(&g, 'G', Dir::Down), Some('I'), "nothing in G's column: nearest ahead");
        assert_eq!(go(&g, 'H', Dir::Down), Some('J'));
        assert_eq!(go(&g, 'I', Dir::Up), Some('D'));
        assert_eq!(go(&g, 'D', Dir::Right), Some('E'), "E is nearer than G within D's rows");
        assert_eq!(go(&g, 'A', Dir::Up), None);
    }

    #[test]
    fn gaps_are_skipped_within_the_band() {
        assert_eq!(go(&game("A..B"), 'A', Dir::Right), Some('B'));
        assert_eq!(go(&game("A..B"), 'B', Dir::Left), Some('A'));
    }

    #[test]
    fn straight_ahead_beats_closer_diagonals() {
        // From A going right: B is in A's row but far; C is near but off to
        // the side. The band rule takes B.
        let g = game("A....B\n.C....");
        assert_eq!(go(&g, 'A', Dir::Right), Some('B'));
        // From C going up there is nothing in C's column; A (one up, one
        // left) beats B (one up, four right).
        assert_eq!(go(&g, 'C', Dir::Up), Some('A'));
    }

    #[test]
    fn irregular_blocks_use_all_their_cells() {
        // L-shaped A reaches into the bottom row, so B is in its band.
        let g = game("A..\nAAB");
        assert_eq!(go(&g, 'A', Dir::Right), Some('B'));
        assert_eq!(go(&g, 'B', Dir::Left), Some('A'));
    }
}
