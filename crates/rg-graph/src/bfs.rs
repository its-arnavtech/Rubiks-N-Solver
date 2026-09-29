//! Layered BFS over an indexed graph, producing an exact distance table
//! (a pattern database / pruning table, docs/06 §5.1).

/// Marker for vertices not reached from any goal.
pub const UNREACHED: u8 = u8::MAX;

#[derive(Clone, Debug)]
pub struct DistanceTable {
    dist: Vec<u8>,
    histogram: Vec<u64>,
}

impl DistanceTable {
    /// Multi-source BFS from `goals` over vertices `0..size`. `neighbor(v, k)` follows
    /// edge `k` (for `k < num_moves`). The graph must be undirected (every move has an
    /// inverse in the move set), which the backward sweep relies on.
    pub fn build(
        size: usize,
        goals: &[u32],
        num_moves: usize,
        neighbor: impl Fn(u32, usize) -> u32,
    ) -> Self {
        let mut dist = vec![UNREACHED; size];
        let mut filled = 0usize;
        for &g in goals {
            if dist[g as usize] == UNREACHED {
                dist[g as usize] = 0;
                filled += 1;
            }
        }
        let mut histogram = vec![filled as u64];
        let mut depth = 0u8;
        loop {
            // Once most vertices are known, checking the unknown ones is cheaper.
            let backward = filled > size / 2;
            let mut added = 0usize;
            for v in 0..size {
                if backward {
                    if dist[v] != UNREACHED {
                        continue;
                    }
                    if (0..num_moves).any(|k| dist[neighbor(v as u32, k) as usize] == depth) {
                        dist[v] = depth + 1;
                        added += 1;
                    }
                } else if dist[v] == depth {
                    for k in 0..num_moves {
                        let w = neighbor(v as u32, k) as usize;
                        if dist[w] == UNREACHED {
                            dist[w] = depth + 1;
                            added += 1;
                        }
                    }
                }
            }
            if added == 0 {
                break;
            }
            filled += added;
            histogram.push(added as u64);
            depth += 1;
            assert!(depth < UNREACHED - 1, "graph too deep for u8 distances");
        }
        Self { dist, histogram }
    }

    /// Frontier (queue) BFS: each vertex is expanded exactly once, so this wins whenever
    /// computing a neighbour is expensive (decode, permute, rank) rather than a table lookup.
    /// `expand(v, push)` must report every neighbour of `v`.
    pub fn build_queued(
        size: usize,
        goals: &[u32],
        mut expand: impl FnMut(u32, &mut dyn FnMut(u32)),
    ) -> Self {
        let mut dist = vec![UNREACHED; size];
        let mut frontier: Vec<u32> = Vec::new();
        for &g in goals {
            if dist[g as usize] == UNREACHED {
                dist[g as usize] = 0;
                frontier.push(g);
            }
        }
        let mut histogram = vec![frontier.len() as u64];
        let mut depth = 0u8;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for &v in &frontier {
                expand(v, &mut |w| {
                    if dist[w as usize] == UNREACHED {
                        dist[w as usize] = depth + 1;
                        next.push(w);
                    }
                });
            }
            if next.is_empty() {
                break;
            }
            histogram.push(next.len() as u64);
            frontier = next;
            depth += 1;
            assert!(depth < UNREACHED - 1, "graph too deep for u8 distances");
        }
        Self { dist, histogram }
    }

    #[inline]
    pub fn get(&self, v: usize) -> u8 {
        self.dist[v]
    }

    pub fn len(&self) -> usize {
        self.dist.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dist.is_empty()
    }

    /// Number of vertices at each distance 0, 1, 2, ...
    pub fn histogram(&self) -> &[u64] {
        &self.histogram
    }

    /// Largest finite distance (the eccentricity of the goal set).
    pub fn max_depth(&self) -> usize {
        self.histogram.len() - 1
    }

    pub fn reached(&self) -> u64 {
        self.histogram.iter().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_graph_distances() {
        // Cycle of 10 vertices: distances from 0 are 0,1,2,3,4,5,4,3,2,1.
        let t = DistanceTable::build(10, &[0], 2, |v, k| {
            if k == 0 { (v + 1) % 10 } else { (v + 9) % 10 }
        });
        let d: Vec<u8> = (0..10).map(|v| t.get(v)).collect();
        assert_eq!(d, [0, 1, 2, 3, 4, 5, 4, 3, 2, 1]);
        assert_eq!(t.histogram(), &[1, 2, 2, 2, 2, 1]);
    }

    #[test]
    fn hypercube_layers_are_binomial() {
        // 10-dimensional hypercube: layer d has C(10, d) vertices.
        let t = DistanceTable::build(1 << 10, &[0], 10, |v, k| v ^ (1 << k));
        let expected: Vec<u64> = (0..=10)
            .map(|d| crate::combinatorics::binomial(10, d))
            .collect();
        assert_eq!(t.histogram(), expected.as_slice());
    }

    #[test]
    fn unreachable_vertices_stay_unreached() {
        // Two disjoint 3-cycles; BFS from 0 never reaches {3, 4, 5}.
        let t = DistanceTable::build(6, &[0], 1, |v, _| {
            if v < 3 { (v + 1) % 3 } else { 3 + (v + 1) % 3 }
        });
        assert_eq!(t.reached(), 3);
        assert_eq!(t.get(4), UNREACHED);
    }
}
