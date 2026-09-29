//! 2x2: the one Rubik's graph small enough to store completely (docs/07 §0).
//!
//! With the DBL corner fixed, every position is a vertex `perm7 · 729 + twist6` in a graph
//! of 3,674,160 vertices and degree 9 (U, R, F). One BFS from solved gives the exact
//! distance of every vertex; solving is then greedy descent, which is always optimal.

use rg_cube::{Geometry, Move};
use rg_graph::bfs::DistanceTable;
use rg_graph::combinatorics::{perm_rank, perm_unrank};

use crate::cubie::{CubieCube, DBL, InvalidCube, corners_from_facelets};

pub const VERTICES: usize = 5040 * 729;
const TWISTS: usize = 729;
const MOVES: usize = 9;
/// Slots other than the fixed DBL corner.
const FREE: [usize; 7] = [0, 1, 2, 3, 4, 5, 7];

fn corner_cube(cp: [u8; 8], co: [u8; 8]) -> CubieCube {
    CubieCube {
        cp,
        co,
        ..CubieCube::SOLVED
    }
}

fn encode(c: &CubieCube) -> u32 {
    let rel = |x: u8| if x == 7 { 6 } else { x };
    let perm = FREE.map(|i| rel(c.cp[i]));
    let twist = c.co[..6].iter().fold(0u32, |a, &o| a * 3 + u32::from(o));
    perm_rank(&perm) * TWISTS as u32 + twist
}

fn decode(v: u32) -> CubieCube {
    let (perm, mut twist) = (v / TWISTS as u32, v % TWISTS as u32);
    let mut c = CubieCube::SOLVED;
    for (k, p) in perm_unrank(perm, 7).into_iter().enumerate() {
        c.cp[FREE[k]] = if p == 6 { 7 } else { p };
    }
    let mut sum = 0;
    for i in (0..6).rev() {
        c.co[i] = (twist % 3) as u8;
        sum += c.co[i];
        twist /= 3;
    }
    c.co[DBL] = 0;
    c.co[7] = (3 - sum % 3) % 3;
    c
}

pub struct Pocket {
    moves: Vec<Move>,
    perm_mv: Vec<u16>,
    twist_mv: Vec<u16>,
    dist: DistanceTable,
}

impl Pocket {
    pub fn new() -> Self {
        let geom = Geometry::new(2);
        let moves = rg_cube::parse_alg(2, "U U2 U' R R2 R' F F2 F'").expect("valid notation");
        let cubies: Vec<CubieCube> = moves
            .iter()
            .map(|&m| {
                let mut c = rg_cube::Cube::new(2).expect("2x2");
                c.apply_move(m);
                let (cp, co) = corners_from_facelets(&geom, c.facelets()).expect("valid");
                corner_cube(cp, co)
            })
            .collect();

        // Permutation and twist evolve independently, so each gets a small move table.
        let mut perm_mv = vec![0u16; 5040 * MOVES];
        let mut twist_mv = vec![0u16; TWISTS * MOVES];
        for (m, mc) in cubies.iter().enumerate() {
            for p in 0..5040 {
                let next = decode(p * TWISTS as u32).mul(mc);
                perm_mv[p as usize * MOVES + m] = (encode(&next) / TWISTS as u32) as u16;
            }
            for t in 0..TWISTS as u32 {
                let next = decode(t).mul(mc);
                twist_mv[t as usize * MOVES + m] = (encode(&next) % TWISTS as u32) as u16;
            }
        }
        let neighbor = |v: u32, m: usize| {
            let (p, t) = (v as usize / TWISTS, v as usize % TWISTS);
            u32::from(perm_mv[p * MOVES + m]) * TWISTS as u32 + u32::from(twist_mv[t * MOVES + m])
        };
        let dist = DistanceTable::build(VERTICES, &[0], MOVES, neighbor);
        Self {
            moves,
            perm_mv,
            twist_mv,
            dist,
        }
    }

    fn neighbor(&self, v: u32, m: usize) -> u32 {
        let (p, t) = (v as usize / TWISTS, v as usize % TWISTS);
        u32::from(self.perm_mv[p * MOVES + m]) * TWISTS as u32
            + u32::from(self.twist_mv[t * MOVES + m])
    }

    pub fn table(&self) -> &DistanceTable {
        &self.dist
    }

    /// Optimal solution for a 2x2 whose DBL corner is home and oriented.
    pub fn solve(&self, geom: &Geometry, facelets: &[u8]) -> Result<Vec<Move>, InvalidCube> {
        let (cp, co) = corners_from_facelets(geom, facelets)?;
        if cp[DBL] != DBL as u8 || co[DBL] != 0 {
            return Err(InvalidCube::NoReferenceCorner);
        }
        let mut v = encode(&corner_cube(cp, co));
        let mut out = Vec::new();
        while self.dist.get(v as usize) > 0 {
            let d = self.dist.get(v as usize);
            let m = (0..MOVES)
                .find(|&m| self.dist.get(self.neighbor(v, m) as usize) == d - 1)
                .expect("a BFS layer always has a neighbour one step closer");
            out.push(self.moves[m]);
            v = self.neighbor(v, m);
        }
        Ok(out)
    }
}

impl Default for Pocket {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_round_trips() {
        for v in (0..VERTICES as u32).step_by(997) {
            assert_eq!(encode(&decode(v)), v);
        }
    }

    #[test]
    fn whole_graph_matches_the_published_distribution() {
        let p = Pocket::new();
        // Known HTM distance distribution of the 2x2x2 (God's number 11).
        assert_eq!(
            p.table().histogram(),
            &[
                1, 9, 54, 321, 1847, 9992, 50136, 227536, 870072, 1887748, 623800, 2644
            ]
        );
        assert_eq!(p.table().reached(), VERTICES as u64);
    }
}
