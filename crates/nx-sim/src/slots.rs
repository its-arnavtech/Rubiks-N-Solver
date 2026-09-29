//! Canonical slot maps (CONVENTIONS §4): which stickers make up each slot of an orbit, and
//! the slot contents read from a labeled cube.

use crate::cube::LabeledCube;
use crate::geometry::{self, B, D, F, L, P3, R, U, sticker_index};
use crate::orbits::{Orbit, OrbitKind};

/// Outward normal of each face (U R F D L B).
const NORMALS: [P3; 6] = [
    [0, 1, 0],
    [1, 0, 0],
    [0, 0, 1],
    [0, -1, 0],
    [-1, 0, 0],
    [0, 0, -1],
];

/// Wing edge order `e = 0..12` as `(f1, f2)`: UB UR UF UL FR FL BR BL DF DR DB DL.
pub const WING_EDGES: [(usize, usize); 12] = [
    (U, B),
    (U, R),
    (U, F),
    (U, L),
    (F, R),
    (F, L),
    (B, R),
    (B, L),
    (D, F),
    (D, R),
    (D, B),
    (D, L),
];

/// Corner slots URF UFL ULB UBR DFR DLF DBL DRB, faces in Kociemba facelet order.
pub const CORNER_SLOTS: [[usize; 3]; 8] = [
    [U, R, F],
    [U, F, L],
    [U, L, B],
    [U, B, R],
    [D, F, R],
    [D, L, F],
    [D, B, L],
    [D, R, B],
];

/// The fixed corner's slot (DBL).
pub const FIXED_CORNER_SLOT: usize = 6;

/// Middle-edge slots UR UF UL UB DR DF DL DB FR FL BL BR, faces in Kociemba facelet order.
pub const MID_EDGE_SLOTS: [[usize; 2]; 12] = [
    [U, R],
    [U, F],
    [U, L],
    [U, B],
    [D, R],
    [D, F],
    [D, L],
    [D, B],
    [F, R],
    [F, L],
    [B, L],
    [B, R],
];

/// The stickers of every slot of one orbit. Slot `i` owns
/// `stickers[i * width..(i + 1) * width]`, in the order the conventions name them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotMap {
    pub kind: OrbitKind,
    pub width: usize,
    pub stickers: Vec<u32>,
}

impl SlotMap {
    pub fn new(n: u32, kind: OrbitKind, a: u32, b: u32) -> Self {
        let width = kind.stickers_per_slot();
        let stickers = match kind {
            OrbitKind::Corner => corner_stickers(n),
            OrbitKind::MidEdge => mid_edge_stickers(n),
            OrbitKind::FixedCenter => {
                let mid = (n - 1) / 2;
                (0..6)
                    .map(|f| sticker_index(n, f, mid, mid) as u32)
                    .collect()
            }
            OrbitKind::Wing => wing_stickers(n, a),
            _ => center_stickers(n, a, b),
        };
        debug_assert_eq!(stickers.len(), kind.slot_count() * width);
        Self {
            kind,
            width,
            stickers,
        }
    }

    pub fn for_orbit(n: u32, orbit: &Orbit) -> Self {
        Self::new(n, orbit.kind, orbit.a, orbit.b)
    }

    pub fn slot_count(&self) -> usize {
        self.stickers.len() / self.width
    }

    pub fn slot(&self, i: usize) -> &[u32] {
        &self.stickers[i * self.width..(i + 1) * self.width]
    }

    /// Slot contents read from a labeled cube (CONVENTIONS §4): `piece·3 + ori` for corners,
    /// `piece·2 + ori` for middle edges, the home slot for wings, the home color for centers.
    /// Panics if a slot holds a sticker from another orbit.
    pub fn extract_labeled(&self, cube: &LabeledCube) -> Vec<u8> {
        let n = cube.n();
        let per_face = (n as usize).pow(2);
        let f = cube.facelets();
        if self.kind.is_center() {
            return self
                .stickers
                .iter()
                .map(|&s| (f[s as usize] as usize / per_face) as u8)
                .collect();
        }
        let home_of = |label: u32| {
            let at = self
                .stickers
                .iter()
                .position(|&s| s == label)
                .expect("sticker from this orbit");
            (at / self.width, at % self.width)
        };
        (0..self.slot_count())
            .map(|i| {
                let slot = self.slot(i);
                let (piece, _) = home_of(f[slot[0] as usize]);
                let content = match self.kind {
                    OrbitKind::Wing => piece,
                    _ => {
                        // Orientation = where the piece's first (U/D or reference) sticker sits.
                        let ori = slot
                            .iter()
                            .position(|&s| home_of(f[s as usize]).1 == 0)
                            .expect("piece's reference sticker is in the slot");
                        piece * self.width + ori
                    }
                };
                content as u8
            })
            .collect()
    }
}

/// Home content of slot `i` (the solved state): `i · ori_mod` for pieces, the face color
/// `i / 4` for 24-slot centers, `i` for the fixed-center frame.
pub fn solved_content(kind: OrbitKind, i: usize) -> u8 {
    let v = match kind {
        OrbitKind::Corner => 3 * i,
        OrbitKind::MidEdge => 2 * i,
        OrbitKind::Wing | OrbitKind::FixedCenter => i,
        _ => i / 4,
    };
    v as u8
}

/// Per-orbit solved check on extracted contents.
pub fn is_orbit_solved(kind: OrbitKind, content: &[u8]) -> bool {
    content
        .iter()
        .enumerate()
        .all(|(i, &c)| c == solved_content(kind, i))
}

/// Sticker of the cubie at `cubie` that lies on `face`.
fn sticker_on(n: u32, cubie: P3, face: usize) -> u32 {
    let axis = NORMALS[face]
        .iter()
        .position(|&v| v != 0)
        .expect("unit normal");
    let mut p = cubie;
    p[axis] = NORMALS[face][axis] * i64::from(n);
    geometry::sticker_at(n, p).expect("cubie has a sticker on this face") as u32
}

fn corner_stickers(n: u32) -> Vec<u32> {
    let m = i64::from(n) - 1;
    CORNER_SLOTS
        .iter()
        .flat_map(|faces| {
            let mut cubie = [0; 3];
            for &f in faces {
                for (c, v) in cubie.iter_mut().zip(NORMALS[f]) {
                    *c += v * m;
                }
            }
            faces.map(|f| sticker_on(n, cubie, f))
        })
        .collect()
}

fn mid_edge_stickers(n: u32) -> Vec<u32> {
    let m = i64::from(n) - 1;
    MID_EDGE_SLOTS
        .iter()
        .flat_map(|faces| {
            let mut cubie = [0; 3];
            for &f in faces {
                for (c, v) in cubie.iter_mut().zip(NORMALS[f]) {
                    *c += v * m;
                }
            }
            faces.map(|f| sticker_on(n, cubie, f))
        })
        .collect()
}

/// Non-corner stickers of face `f1` on its border with `f2`, ascending (position t = 1..N−2).
pub fn edge_run(n: u32, f1: usize, f2: usize) -> Vec<u32> {
    let m = n - 1;
    let axis2 = NORMALS[f2]
        .iter()
        .position(|&v| v != 0)
        .expect("unit normal");
    let target = NORMALS[f2][axis2] * i64::from(m);
    let mut run: Vec<u32> = (1..m)
        .flat_map(|i| [(0, i), (m, i), (i, 0), (i, m)])
        .map(|(r, c)| sticker_index(n, f1, r, c) as u32)
        .filter(|&s| geometry::cubie_of(n, geometry::position_of(n, s as usize))[axis2] == target)
        .collect();
    run.sort_unstable();
    run.dedup();
    debug_assert_eq!(run.len(), (n - 2) as usize);
    run
}

fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Wing slots `2e + s`. `s` is the member's handedness (CONVENTIONS §4): `s = 0` when
/// `(n_f1 × n_f2) · cubie > 0`. Rotations preserve this triple product, which is what makes
/// `(f1 color, f2 color, s)` determine a wing's identity (CONVENTIONS §5).
fn wing_stickers(n: u32, p: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(48);
    for (f1, f2) in WING_EDGES {
        let run = edge_run(n, f1, f2);
        let axis = cross(NORMALS[f1], NORMALS[f2]);
        let mut members: Vec<(bool, [u32; 2])> = [p, n - 1 - p]
            .into_iter()
            .map(|t| {
                let s1 = run[(t - 1) as usize];
                let cubie = geometry::cubie_of(n, geometry::position_of(n, s1 as usize));
                let dot: i64 = axis.iter().zip(cubie).map(|(a, c)| a * c).sum();
                (dot < 0, [s1, sticker_on(n, cubie, f2)])
            })
            .collect();
        members.sort_by_key(|m| m.0);
        out.extend(members.into_iter().flat_map(|m| m.1));
    }
    out
}

fn center_stickers(n: u32, a: u32, b: u32) -> Vec<u32> {
    let m = n - 1;
    let mut out = Vec::with_capacity(24);
    for face in 0..6 {
        let (mut r, mut c) = (a, b);
        for _ in 0..4 {
            out.push(sticker_index(n, face, r, c) as u32);
            (r, c) = (c, m - r);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbits::orbits;
    use crate::rng::{below, rng};
    use crate::scramble_moves;

    #[test]
    fn slot_maps_are_bijections_onto_their_orbit() {
        for n in 2..=20 {
            for o in orbits(n) {
                let map = SlotMap::for_orbit(n, &o);
                let mut s = map.stickers.clone();
                s.sort_unstable();
                assert_eq!(s, o.stickers, "n={n} {} ({}, {})", o.kind, o.a, o.b);
            }
        }
    }

    #[test]
    fn solved_extracts_to_identity_and_contents_stay_valid() {
        let mut r = rng(9);
        for n in 2..=11 {
            let os = orbits(n);
            let maps: Vec<SlotMap> = os.iter().map(|o| SlotMap::for_orbit(n, o)).collect();
            let solved = LabeledCube::solved(n);
            for map in &maps {
                assert!(
                    is_orbit_solved(map.kind, &map.extract_labeled(&solved)),
                    "n={n}"
                );
            }
            let mut c = LabeledCube::solved(n);
            c.apply_all(&scramble_moves(n, 200, below(&mut r, 1000)));
            for map in &maps {
                let content = map.extract_labeled(&c);
                match map.kind {
                    OrbitKind::Corner | OrbitKind::MidEdge | OrbitKind::Wing => {
                        let w = if map.kind == OrbitKind::Wing {
                            1
                        } else {
                            map.width as u8
                        };
                        let mut pieces: Vec<u8> = content.iter().map(|&x| x / w).collect();
                        pieces.sort_unstable();
                        assert_eq!(pieces, (0..map.slot_count() as u8).collect::<Vec<_>>());
                        let ori: u32 = content.iter().map(|&x| u32::from(x % w)).sum();
                        assert_eq!(ori % u32::from(w), 0, "n={n} {} orientation sum", map.kind);
                    }
                    _ => {
                        let mut colors = content.clone();
                        colors.sort_unstable();
                        let per = map.slot_count() / 6;
                        let want: Vec<u8> =
                            (0..6u8).flat_map(|f| std::iter::repeat_n(f, per)).collect();
                        assert_eq!(colors, want, "n={n} {} colors", map.kind);
                    }
                }
                if map.kind == OrbitKind::Corner {
                    assert_eq!(content[FIXED_CORNER_SLOT], 3 * FIXED_CORNER_SLOT as u8);
                }
            }
        }
    }

    #[test]
    fn slots_have_the_named_faces() {
        let n = 6;
        let face = |s: u32| geometry::sticker_coords(n, s as usize).0;
        let corner = SlotMap::new(n, OrbitKind::Corner, 0, 0);
        for (i, faces) in CORNER_SLOTS.iter().enumerate() {
            assert_eq!(
                corner.slot(i).iter().map(|&s| face(s)).collect::<Vec<_>>(),
                faces
            );
        }
        let wing = SlotMap::new(n, OrbitKind::Wing, 2, 0);
        for (e, (f1, f2)) in WING_EDGES.iter().enumerate() {
            for s in 0..2 {
                let slot = wing.slot(2 * e + s);
                assert_eq!((face(slot[0]), face(slot[1])), (*f1, *f2));
            }
        }
        // Center slot 4f + k is rot^k of (a, b) on face f.
        let x = SlotMap::new(n, OrbitKind::ObliqueA, 1, 2);
        assert_eq!(
            geometry::sticker_coords(n, x.slot(9)[0] as usize),
            (2, 2, 4)
        );
    }
}
