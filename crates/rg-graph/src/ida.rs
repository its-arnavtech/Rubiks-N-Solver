//! Depth-bounded search on implicit graphs with admissible heuristics (IDA*, docs/06 §6).

use std::cell::Cell;

/// An implicit graph searched towards a goal set.
pub trait SearchSpace {
    type Node: Copy;
    /// Edge labels (move ids) available in this space.
    fn moves(&self) -> &[u8];
    fn step(&self, node: Self::Node, mv: u8) -> Self::Node;
    /// Admissible lower bound on the distance to the goal; 0 exactly at goal nodes.
    fn h(&self, node: Self::Node) -> u8;
    /// Canonical-sequence filter: may `mv` follow `prev`?
    fn allowed(&self, prev: Option<u8>, mv: u8) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Stop,
}

/// Shared search bookkeeping: node counter and a time limit that can be armed once a
/// first solution exists (so a search never returns empty-handed).
pub struct SearchCtl<'a> {
    nodes: Cell<u64>,
    aborted: Cell<bool>,
    armed: Cell<bool>,
    time_up: &'a dyn Fn() -> bool,
}

impl<'a> SearchCtl<'a> {
    pub fn new(time_up: &'a dyn Fn() -> bool) -> Self {
        Self {
            nodes: Cell::new(0),
            aborted: Cell::new(false),
            armed: Cell::new(false),
            time_up,
        }
    }

    /// A control whose time limit applies from the start (no first solution to wait for).
    pub fn with_deadline(time_up: &'a dyn Fn() -> bool) -> Self {
        let ctl = Self::new(time_up);
        ctl.arm();
        ctl
    }

    pub fn nodes(&self) -> u64 {
        self.nodes.get()
    }

    pub fn aborted(&self) -> bool {
        self.aborted.get()
    }

    /// Enforce the time limit from now on.
    pub fn arm(&self) {
        self.armed.set(true);
    }

    #[inline]
    fn tick(&self) -> bool {
        let n = self.nodes.get() + 1;
        self.nodes.set(n);
        if n & 0xFFF == 0 && self.armed.get() && (self.time_up)() {
            self.aborted.set(true);
        }
        self.aborted.get()
    }
}

/// Enumerate canonical paths of exactly `depth` moves from `start` that end at a goal,
/// pruning any node whose heuristic exceeds the remaining depth. `on_goal` receives each
/// path and decides whether to keep enumerating.
pub fn search_exact<S: SearchSpace>(
    space: &S,
    start: S::Node,
    depth: u8,
    prev: Option<u8>,
    path: &mut Vec<u8>,
    ctl: &SearchCtl<'_>,
    on_goal: &mut dyn FnMut(&[u8]) -> Flow,
) -> Flow {
    if depth == 0 {
        return if space.h(start) == 0 {
            on_goal(path)
        } else {
            Flow::Continue
        };
    }
    for &mv in space.moves() {
        if !space.allowed(prev, mv) {
            continue;
        }
        if ctl.tick() {
            return Flow::Stop;
        }
        let child = space.step(start, mv);
        if space.h(child) >= depth {
            continue;
        }
        path.push(mv);
        let flow = search_exact(space, child, depth - 1, Some(mv), path, ctl, on_goal);
        path.pop();
        if flow == Flow::Stop {
            return Flow::Stop;
        }
    }
    Flow::Continue
}

enum Outcome {
    Found,
    NotFound,
    Aborted,
}

/// Classic IDA* with an f = g + h threshold that accepts a goal at *any* depth within the
/// bound, raising the bound to the smallest pruned f each pass. With an admissible h this
/// is ordinary IDA*; with an inflated h (weighted IDA*) it dives towards low-h regions and
/// finds good, not necessarily shortest, paths much faster.
pub fn ida_star_any_depth<S: SearchSpace>(
    space: &S,
    start: S::Node,
    max_bound: u8,
    prev: Option<u8>,
    ctl: &SearchCtl<'_>,
) -> Option<Vec<u8>> {
    // Internal recursion: the parameters are the search state, passed down unchanged or
    // one step further; a struct would only rename them.
    #[allow(clippy::too_many_arguments)]
    fn dfs<S: SearchSpace>(
        space: &S,
        node: S::Node,
        h: u8,
        g: u16,
        bound: u16,
        prev: Option<u8>,
        path: &mut Vec<u8>,
        ctl: &SearchCtl<'_>,
        next: &mut u16,
    ) -> Outcome {
        if h == 0 {
            return Outcome::Found;
        }
        for &mv in space.moves() {
            if !space.allowed(prev, mv) {
                continue;
            }
            if ctl.tick() {
                return Outcome::Aborted;
            }
            let child = space.step(node, mv);
            let hc = space.h(child);
            let f = g + 1 + u16::from(hc);
            if f > bound {
                *next = (*next).min(f);
                continue;
            }
            path.push(mv);
            match dfs(space, child, hc, g + 1, bound, Some(mv), path, ctl, next) {
                Outcome::NotFound => {
                    path.pop();
                }
                other => return other,
            }
        }
        Outcome::NotFound
    }

    let h0 = space.h(start);
    let mut bound = u16::from(h0);
    let mut path = Vec::new();
    while bound <= u16::from(max_bound) {
        let mut next = u16::MAX;
        match dfs(space, start, h0, 0, bound, prev, &mut path, ctl, &mut next) {
            Outcome::Found => return Some(path),
            Outcome::Aborted => return None,
            Outcome::NotFound => {}
        }
        if next == u16::MAX {
            return None;
        }
        bound = next;
    }
    None
}

/// IDA*: the shortest canonical path to a goal, up to `max_depth` moves.
pub fn ida_star<S: SearchSpace>(
    space: &S,
    start: S::Node,
    max_depth: u8,
    prev: Option<u8>,
    ctl: &SearchCtl<'_>,
) -> Option<Vec<u8>> {
    let mut path = Vec::new();
    for depth in space.h(start)..=max_depth {
        let mut found = None;
        search_exact(space, start, depth, prev, &mut path, ctl, &mut |p| {
            found = Some(p.to_vec());
            Flow::Stop
        });
        if found.is_some() || ctl.aborted() {
            return found;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bfs::DistanceTable;

    /// Sliding 8-puzzle: node = permutation rank of the 9 tiles (0 = blank).
    struct Eight {
        table: DistanceTable,
    }

    fn neighbors(state: &[u8]) -> [Option<Vec<u8>>; 4] {
        let b = state.iter().position(|&t| t == 0).unwrap();
        let (r, c) = (b / 3, b % 3);
        let swap = |ok: bool, o: usize| {
            ok.then(|| {
                let mut s = state.to_vec();
                s.swap(b, o);
                s
            })
        };
        [
            swap(r > 0, b.wrapping_sub(3)),
            swap(r < 2, b + 3),
            swap(c > 0, b.wrapping_sub(1)),
            swap(c < 2, b + 1),
        ]
    }

    impl SearchSpace for Eight {
        type Node = u32;
        fn moves(&self) -> &[u8] {
            &[0, 1, 2, 3]
        }
        fn step(&self, node: u32, mv: u8) -> u32 {
            let s = crate::combinatorics::perm_unrank(node, 9);
            neighbors(&s)[mv as usize]
                .as_ref()
                .map_or(node, |n| crate::combinatorics::perm_rank(n))
        }
        fn h(&self, node: u32) -> u8 {
            self.table.get(node as usize)
        }
        fn allowed(&self, prev: Option<u8>, mv: u8) -> bool {
            prev != Some(mv ^ 1)
        }
    }

    #[test]
    fn ida_with_exact_heuristic_finds_optimal_paths() {
        let neighbor = |v: u32, k: usize| {
            let s = crate::combinatorics::perm_unrank(v, 9);
            neighbors(&s)[k]
                .as_ref()
                .map_or(v, |n| crate::combinatorics::perm_rank(n))
        };
        let table = DistanceTable::build(362_880, &[0], 4, neighbor);
        assert_eq!(table.reached(), 181_440); // half the permutations are solvable
        assert_eq!(table.max_depth(), 31); // known 8-puzzle diameter
        let space = Eight { table };
        let never = || false;
        let ctl = SearchCtl::new(&never);
        for start in [1u32, 777, 4242, 100_000, 181_000] {
            let d = space.table.get(start as usize);
            if d == crate::bfs::UNREACHED {
                continue;
            }
            let path = ida_star(&space, start, 40, None, &ctl).unwrap();
            assert_eq!(path.len(), d as usize);
        }
    }
}
