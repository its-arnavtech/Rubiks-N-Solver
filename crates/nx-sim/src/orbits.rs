//! Orbit computation and classification (ARCHITECTURE §5, CONVENTIONS §3–4).
//!
//! Orbits are computed, not hand-coded: union-find over the quarter-turn cycles of every layer
//! on every axis, after joining the stickers of each physical piece. Layer N−1 is included so
//! the fixed D-L-B corner lands in the corner orbit; it adds no other merges, because every
//! other orbit is already closed under the allowed moves.

use std::collections::HashMap;
use std::fmt;

use crate::cube::Layout;
use crate::geometry::{self, sticker_coords, sticker_count};
use crate::moves::Axis;

/// Orbit kinds, declared in the id order of CONVENTIONS §3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OrbitKind {
    Corner,
    MidEdge,
    FixedCenter,
    Wing,
    XCenter,
    PlusCenter,
    ObliqueA,
    ObliqueB,
}

impl OrbitKind {
    pub const ALL: [OrbitKind; 8] = [
        OrbitKind::Corner,
        OrbitKind::MidEdge,
        OrbitKind::FixedCenter,
        OrbitKind::Wing,
        OrbitKind::XCenter,
        OrbitKind::PlusCenter,
        OrbitKind::ObliqueA,
        OrbitKind::ObliqueB,
    ];

    /// Network type id (CONVENTIONS §3). `FixedCenter` has none: code handles it.
    pub const fn type_id(self) -> Option<u8> {
        match self {
            OrbitKind::Corner => Some(0),
            OrbitKind::MidEdge => Some(1),
            OrbitKind::Wing => Some(2),
            OrbitKind::XCenter => Some(3),
            OrbitKind::PlusCenter => Some(4),
            OrbitKind::ObliqueA => Some(5),
            OrbitKind::ObliqueB => Some(6),
            OrbitKind::FixedCenter => None,
        }
    }

    pub fn from_type_id(id: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.type_id() == Some(id))
    }

    pub const fn name(self) -> &'static str {
        match self {
            OrbitKind::Corner => "Corner",
            OrbitKind::MidEdge => "MidEdge",
            OrbitKind::FixedCenter => "FixedCenter",
            OrbitKind::Wing => "Wing",
            OrbitKind::XCenter => "XCenter",
            OrbitKind::PlusCenter => "PlusCenter",
            OrbitKind::ObliqueA => "ObliqueA",
            OrbitKind::ObliqueB => "ObliqueB",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Canonical slots per orbit (CONVENTIONS §4).
    pub const fn slot_count(self) -> usize {
        match self {
            OrbitKind::Corner => 8,
            OrbitKind::MidEdge => 12,
            OrbitKind::FixedCenter => 6,
            _ => 24,
        }
    }

    /// Stickers per slot: the piece's sticker count.
    pub const fn stickers_per_slot(self) -> usize {
        match self {
            OrbitKind::Corner => 3,
            OrbitKind::MidEdge | OrbitKind::Wing => 2,
            _ => 1,
        }
    }

    /// Color types: content is a color; pieces of one color are interchangeable.
    pub const fn is_center(self) -> bool {
        matches!(
            self,
            OrbitKind::XCenter
                | OrbitKind::PlusCenter
                | OrbitKind::ObliqueA
                | OrbitKind::ObliqueB
                | OrbitKind::FixedCenter
        )
    }
}

impl fmt::Display for OrbitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One orbit instance. Indices: `Wing` `a = p`; `XCenter` `a = b`; `PlusCenter` `b = MID`;
/// obliques `(a, b)`; `a = b = 0` for `Corner`, `MidEdge`, `FixedCenter`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Orbit {
    pub id: u32,
    pub kind: OrbitKind,
    pub a: u32,
    pub b: u32,
    /// Sticker indices, ascending.
    pub stickers: Vec<u32>,
}

impl Orbit {
    pub fn type_id(&self) -> Option<u8> {
        self.kind.type_id()
    }
}

struct Dsu(Vec<u32>);

impl Dsu {
    fn new(len: usize) -> Self {
        Self((0..len as u32).collect())
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.0[x as usize] != x {
            let p = self.0[x as usize];
            self.0[x as usize] = self.0[p as usize];
            x = p;
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a as u32), self.find(b as u32));
        if ra != rb {
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            self.0[hi as usize] = lo;
        }
    }
}

/// `q = ⌊(N−2)/2⌋`, the number of wing / x-center orbits.
pub fn q_of(n: u32) -> u32 {
    n.saturating_sub(2) / 2
}

/// All orbits of an NxN cube, in id order (CONVENTIONS §3).
pub fn orbits(n: u32) -> Vec<Orbit> {
    assert!(n >= 2, "cube size must be at least 2, got {n}");
    let layout = Layout::new(n);
    let mut dsu = Dsu::new(sticker_count(n));
    for axis in Axis::ALL {
        for layer in 0..n {
            layout.for_each_quarter_cycle(axis, layer, |c| {
                dsu.union(c[0], c[1]);
                dsu.union(c[1], c[2]);
                dsu.union(c[2], c[3]);
            });
        }
    }
    // Join the stickers of each edge and corner piece (they all sit on face borders).
    let m = n - 1;
    let mut cubie_first: HashMap<[i64; 3], usize> = HashMap::new();
    for face in 0..6 {
        for i in 0..n {
            for (r, c) in [(0, i), (m, i), (i, 0), (i, m)] {
                let s = geometry::sticker_index(n, face, r, c);
                let cubie = geometry::cubie_of(n, geometry::position_of(n, s));
                let first = *cubie_first.entry(cubie).or_insert(s);
                dsu.union(first, s);
            }
        }
    }
    let mut groups: HashMap<u32, Vec<u32>> = HashMap::new();
    for s in 0..sticker_count(n) as u32 {
        groups.entry(dsu.find(s)).or_default().push(s);
    }
    let mut out: Vec<Orbit> = groups
        .into_values()
        .map(|stickers| {
            let (kind, a, b) = classify(n, &stickers);
            Orbit {
                id: 0,
                kind,
                a,
                b,
                stickers,
            }
        })
        .collect();
    out.sort_by_key(|o| (o.kind, o.a, o.b));
    for (id, o) in out.iter_mut().enumerate() {
        o.id = id as u32;
    }
    out
}

/// Classify an orbit from its stickers (CONVENTIONS §4).
fn classify(n: u32, stickers: &[u32]) -> (OrbitKind, u32, u32) {
    let m = n - 1;
    let border = |v: u32| v == 0 || v == m;
    let (_, r, c) = sticker_coords(n, stickers[0] as usize);
    match (border(r), border(c)) {
        (true, true) => (OrbitKind::Corner, 0, 0),
        (true, false) | (false, true) => {
            let t = if border(r) { c } else { r };
            if n % 2 == 1 && t == m / 2 {
                (OrbitKind::MidEdge, 0, 0)
            } else {
                (OrbitKind::Wing, t.min(m - t), 0)
            }
        }
        (false, false) => {
            if stickers.len() == 6 {
                return (OrbitKind::FixedCenter, 0, 0);
            }
            let q = q_of(n);
            let mid = (n % 2 == 1).then_some(m / 2);
            let on_u = stickers
                .iter()
                .map(|&s| sticker_coords(n, s as usize))
                .filter(|x| x.0 == 0);
            let mut plus = None;
            for (_, r, c) in on_u {
                if (1..=q).contains(&r) && (1..=q).contains(&c) {
                    let kind = match r.cmp(&c) {
                        std::cmp::Ordering::Equal => OrbitKind::XCenter,
                        std::cmp::Ordering::Less => OrbitKind::ObliqueA,
                        std::cmp::Ordering::Greater => OrbitKind::ObliqueB,
                    };
                    return (kind, r, c);
                }
                if Some(c) == mid && r < c {
                    plus = Some(r);
                }
            }
            let a = plus.expect("center orbit has a representative (CONVENTIONS §4)");
            (OrbitKind::PlusCenter, a, m / 2)
        }
    }
}

/// Orbit counts per kind for N ≥ 2 from the ARCHITECTURE §5 formulas.
pub fn expected_count(n: u32, kind: OrbitKind) -> usize {
    let q = q_of(n) as usize;
    let odd = n % 2 == 1;
    match kind {
        OrbitKind::Corner => 1,
        OrbitKind::MidEdge | OrbitKind::FixedCenter => usize::from(odd && n >= 3),
        OrbitKind::Wing | OrbitKind::XCenter => q,
        OrbitKind::PlusCenter => {
            if odd {
                q
            } else {
                0
            }
        }
        OrbitKind::ObliqueA | OrbitKind::ObliqueB => q * q.saturating_sub(1) / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::LabeledCube;
    use crate::moves::allowed_moves;

    #[test]
    fn counts_match_formulas() {
        for n in 2..=40 {
            let os = orbits(n);
            for kind in OrbitKind::ALL {
                let got = os.iter().filter(|o| o.kind == kind).count();
                assert_eq!(got, expected_count(n, kind), "n={n} {kind}");
            }
            for o in &os {
                let pieces = if o.kind == OrbitKind::FixedCenter {
                    6
                } else {
                    o.kind.slot_count()
                };
                assert_eq!(
                    o.stickers.len(),
                    pieces * o.kind.stickers_per_slot(),
                    "n={n} {o:?}"
                );
            }
        }
    }

    #[test]
    fn every_sticker_in_exactly_one_orbit_and_ids_ordered() {
        for n in 2..=24 {
            let os = orbits(n);
            let mut seen = vec![0u8; sticker_count(n)];
            for (i, o) in os.iter().enumerate() {
                assert_eq!(o.id as usize, i);
                for &s in &o.stickers {
                    seen[s as usize] += 1;
                }
            }
            assert!(seen.iter().all(|&c| c == 1), "n={n}");
            assert!(
                os.windows(2)
                    .all(|w| (w[0].kind, w[0].a, w[0].b) < (w[1].kind, w[1].a, w[1].b))
            );
        }
    }

    #[test]
    fn every_move_maps_each_orbit_to_itself() {
        for n in 2..=13 {
            let os = orbits(n);
            let mut orbit_of = vec![0u32; sticker_count(n)];
            for o in &os {
                for &s in &o.stickers {
                    orbit_of[s as usize] = o.id;
                }
            }
            for m in allowed_moves(n) {
                let mut c = LabeledCube::solved(n);
                c.apply(m);
                for (pos, &home) in c.facelets().iter().enumerate() {
                    assert_eq!(orbit_of[pos], orbit_of[home as usize], "n={n} {m}");
                }
            }
        }
    }

    #[test]
    fn indices_for_n6_and_n7() {
        let kinds = |n| {
            orbits(n)
                .iter()
                .map(|o| (o.kind, o.a, o.b))
                .collect::<Vec<_>>()
        };
        use OrbitKind::*;
        assert_eq!(
            kinds(6),
            vec![
                (Corner, 0, 0),
                (Wing, 1, 0),
                (Wing, 2, 0),
                (XCenter, 1, 1),
                (XCenter, 2, 2),
                (ObliqueA, 1, 2),
                (ObliqueB, 2, 1)
            ]
        );
        assert_eq!(
            kinds(7),
            vec![
                (Corner, 0, 0),
                (MidEdge, 0, 0),
                (FixedCenter, 0, 0),
                (Wing, 1, 0),
                (Wing, 2, 0),
                (XCenter, 1, 1),
                (XCenter, 2, 2),
                (PlusCenter, 1, 3),
                (PlusCenter, 2, 3),
                (ObliqueA, 1, 2),
                (ObliqueB, 2, 1)
            ]
        );
    }
}
