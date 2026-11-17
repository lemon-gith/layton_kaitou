//! What can be read out of a finished (or stopped) search: solutions, how
//! many there are, alternatives, and distances for hints.

use std::collections::{HashMap, HashSet, VecDeque};

use dd_engine::{Move, Position};

use crate::Solver;

/// A sequence of moves from the start to a goal position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Solution {
    pub moves: Vec<Move>,
    /// Position before the first move and after each move.
    pub positions: Vec<Position>,
}

impl Solution {
    pub fn len(&self) -> usize {
        self.moves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }
}

/// Moves-to-goal for every position the search stored.
pub struct DistanceMap {
    dist: Vec<u32>,
    exact: bool,
}

impl DistanceMap {
    /// Whether the distances are true minimums. If the search stopped before
    /// seeing every reachable position they are only upper bounds.
    pub fn is_exact(&self) -> bool {
        self.exact
    }

    /// How many stored positions sit at each distance from the goal.
    pub fn histogram(&self) -> Vec<usize> {
        let mut counts = Vec::new();
        for &d in self.dist.iter().filter(|&&d| d != u32::MAX) {
            if counts.len() <= d as usize {
                counts.resize(d as usize + 1, 0);
            }
            counts[d as usize] += 1;
        }
        counts
    }
}

/// The best next move from some position.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub mv: Move,
    /// Moves left after making this one.
    pub remaining: u32,
}

/// The states and moves that lie on at least one shortest solution.
struct ShortestDag {
    /// State numbers, ascending, so predecessors come first.
    nodes: Vec<u32>,
    /// For each node, `(predecessor index, move label)`.
    preds: Vec<Vec<(usize, u32)>>,
    /// Indices of goal nodes.
    goals: Vec<usize>,
}

impl Solver {
    /// Neighbours of a stored state that are stored too, with the move that
    /// leads to each (in the frame of the stored representative).
    fn neighbours(&self, id: u32) -> Vec<(u32, Move)> {
        let position = self.layout.decode(self.store.key(id)).expect("stored keys decode");
        let mut key = Vec::new();
        let mut out = Vec::new();
        self.layout.for_each_successor(&position, self.options.metric, |mv, next| {
            self.encode(next, &mut key);
            if let Some(other) = self.store.find(&key) {
                out.push((other, mv));
            }
        });
        out
    }

    /// Turns a chain of state numbers into moves on the real board. Stored
    /// states may be mirror images of what is on the board, so each move is
    /// found by matching successors of the real position against the next key.
    fn realise(&self, ids: &[u32]) -> Solution {
        let mut current = self.start;
        let mut moves = Vec::with_capacity(ids.len().saturating_sub(1));
        let mut positions = vec![current];
        let mut key = Vec::new();
        for &id in ids.iter().skip(1) {
            let target = self.store.key(id);
            let mut found = None;
            self.layout.for_each_successor(&current, self.options.metric, |mv, next| {
                if found.is_none() {
                    self.encode(next, &mut key);
                    if key == target {
                        found = Some((mv, *next));
                    }
                }
            });
            let (mv, next) = found.expect("consecutive states are one move apart");
            moves.push(mv);
            positions.push(next);
            current = next;
        }
        Solution { moves, positions }
    }

    /// One shortest solution, if the goal was reached.
    pub fn solution(&self) -> Option<Solution> {
        let mut ids = vec![*self.goals.first()?];
        while let Some(&id) = ids.last().filter(|&&id| id != 0) {
            ids.push(self.store.parent(id));
        }
        ids.reverse();
        Some(self.realise(&ids))
    }

    fn shortest_dag(&self) -> Option<ShortestDag> {
        if self.goals.is_empty() {
            return None;
        }
        let mut labels: HashMap<Move, u32> = HashMap::new();
        let mut edges: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        let mut seen: HashSet<u32> = self.goals.iter().copied().collect();
        let mut todo: Vec<u32> = self.goals.clone();
        while let Some(id) = todo.pop() {
            let depth = self.depth_of(id);
            if depth == 0 {
                continue;
            }
            let mut preds: Vec<(u32, u32)> = Vec::new();
            for (other, mv) in self.neighbours(id) {
                if self.depth_of(other) + 1 != depth || preds.iter().any(|p| p.0 == other) {
                    continue;
                }
                // The move as played towards the goal is the reverse of the
                // one generated from this side.
                let mut played = Move { class: mv.class, from: mv.to, to: mv.from };
                if self.options.symmetry {
                    played = self.layout.canonical_move(played);
                }
                let next_label = labels.len() as u32;
                preds.push((other, *labels.entry(played).or_insert(next_label)));
                if seen.insert(other) {
                    todo.push(other);
                }
            }
            edges.insert(id, preds);
        }

        let mut nodes: Vec<u32> = seen.into_iter().collect();
        nodes.sort_unstable();
        let index = |id: u32| nodes.binary_search(&id).expect("node collected");
        let preds = nodes
            .iter()
            .map(|id| {
                edges
                    .get(id)
                    .map(|list| list.iter().map(|&(p, label)| (index(p), label)).collect())
                    .unwrap_or_default()
            })
            .collect();
        let goals = self.goals.iter().map(|&g| index(g)).collect();
        Some(ShortestDag { nodes, preds, goals })
    }

    /// Number of distinct shortest solutions, counted as sequences of
    /// positions (mirror images counted once when symmetry is on). Saturates
    /// at `u128::MAX`. A lower bound if [`Solver::shortest_is_complete`] is
    /// false.
    pub fn shortest_count(&self) -> Option<u128> {
        let dag = self.shortest_dag()?;
        let mut count = vec![0u128; dag.nodes.len()];
        for i in 0..dag.nodes.len() {
            count[i] = if dag.preds[i].is_empty() {
                1
            } else {
                dag.preds[i]
                    .iter()
                    .fold(0u128, |acc, &(p, _)| acc.saturating_add(count[p]))
            };
        }
        Some(dag.goals.iter().fold(0u128, |acc, &g| acc.saturating_add(count[g])))
    }

    /// Up to `max` shortest solutions chosen to differ from one another.
    ///
    /// Each one after the first is the shortest solution that reuses the
    /// fewest moves of those already chosen. The list ends when the best
    /// remaining candidate is not different enough: fewer than a quarter of
    /// its moves are new, which is what a re-ordering or a small local
    /// variation of an earlier solution looks like.
    pub fn solutions(&self, max: usize) -> Vec<Solution> {
        let Some(dag) = self.shortest_dag() else {
            return Vec::new();
        };
        let mut used: HashMap<u32, u64> = HashMap::new();
        let mut out = Vec::new();
        while out.len() < max {
            // Cheapest path to each node, where a move costs the number of
            // times earlier solutions made it.
            let mut cost = vec![0u64; dag.nodes.len()];
            let mut via = vec![None::<(usize, u32)>; dag.nodes.len()];
            for i in 0..dag.nodes.len() {
                let best = dag.preds[i]
                    .iter()
                    .map(|&(p, label)| (cost[p] + used.get(&label).copied().unwrap_or(0), p, label))
                    .min();
                if let Some((c, p, label)) = best {
                    cost[i] = c;
                    via[i] = Some((p, label));
                }
            }
            let Some(&goal) = dag.goals.iter().min_by_key(|&&g| cost[g]) else {
                break;
            };
            let mut ids = vec![dag.nodes[goal]];
            let mut labels = Vec::new();
            let mut at = goal;
            while let Some((p, label)) = via[at] {
                ids.push(dag.nodes[p]);
                labels.push(label);
                at = p;
            }
            ids.reverse();
            let fresh = labels.iter().filter(|label| !used.contains_key(label)).count();
            if !out.is_empty() && fresh * 4 < labels.len().max(1) {
                break;
            }
            for label in labels {
                *used.entry(label).or_insert(0) += 1;
            }
            out.push(self.realise(&ids));
        }
        out
    }

    /// Distance to the nearest goal for every stored position. Costs about
    /// as much as the search itself.
    pub fn distance_map(&self) -> DistanceMap {
        let mut dist = vec![u32::MAX; self.store.len()];
        let mut queue = VecDeque::new();
        for &goal in &self.all_goals {
            dist[goal as usize] = 0;
            queue.push_back(goal);
        }
        while let Some(id) = queue.pop_front() {
            for (other, _) in self.neighbours(id) {
                if dist[other as usize] == u32::MAX {
                    dist[other as usize] = dist[id as usize] + 1;
                    queue.push_back(other);
                }
            }
        }
        DistanceMap { dist, exact: self.exhausted }
    }

    /// Moves needed from `position`, if the search stored it and it can
    /// reach a goal.
    pub fn distance(&self, map: &DistanceMap, position: &Position) -> Option<u32> {
        let mut key = Vec::new();
        self.encode(position, &mut key);
        let id = self.store.find(&key)?;
        Some(map.dist[id as usize]).filter(|&d| d != u32::MAX)
    }

    /// The move from `position` that gets closest to a goal.
    pub fn hint(&self, map: &DistanceMap, position: &Position) -> Option<Hint> {
        let mut best: Option<Hint> = None;
        self.layout.for_each_successor(position, self.options.metric, |mv, next| {
            if let Some(remaining) = self.distance(map, next) {
                if best.is_none_or(|b| remaining < b.remaining) {
                    best = Some(Hint { mv, remaining });
                }
            }
        });
        best.filter(|_| !self.layout.is_goal(position))
    }
}
