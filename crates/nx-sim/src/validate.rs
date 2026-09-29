//! State validation and uniform random reachable states (CONVENTIONS §6).

use std::fmt;

use crate::cube::Cube;
use crate::identity::{IdentityError, frame_index, frame_parity, frame_rotations, piece_parity};
use crate::orbits::{Orbit, OrbitKind, orbits};
use crate::rng::{below, rng, shuffle};
use crate::slots::{FIXED_CORNER_SLOT, SlotMap};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    /// A color does not appear exactly N² times.
    ColorCount {
        color: u8,
        count: usize,
        expected: usize,
    },
    /// Some slot's colors are no piece of its orbit.
    Piece(IdentityError),
    /// A piece appears twice (Corner, MidEdge, Wing).
    DuplicatePiece { orbit_id: u32, kind: OrbitKind },
    /// The D-L-B corner is not home and untwisted (fixed-corner frame, ADR-002).
    FixedCornerMoved,
    /// Corner twists don't sum to 0 mod 3.
    CornerTwist,
    /// Middle-edge flips don't sum to 0 mod 2.
    EdgeFlip,
    /// A center orbit doesn't hold 4 stickers of each color.
    CenterColors { orbit_id: u32, kind: OrbitKind },
    /// The six face centers are not a rotation of the solved frame.
    Frame,
    /// parity(corners) ⊕ parity(middle edges) ⊕ parity(frame) ≠ 0 (odd N).
    ParityLaw,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ColorCount {
                color,
                count,
                expected,
            } => {
                write!(
                    f,
                    "color {color} appears {count} times, expected {expected}"
                )
            }
            Self::Piece(e) => write!(f, "{e}"),
            Self::DuplicatePiece { orbit_id, kind } => {
                write!(f, "orbit {orbit_id} ({kind}) holds a piece twice")
            }
            Self::FixedCornerMoved => write!(f, "the D-L-B corner is not solved"),
            Self::CornerTwist => write!(f, "corner twists do not sum to 0 mod 3"),
            Self::EdgeFlip => write!(f, "middle-edge flips do not sum to 0 mod 2"),
            Self::CenterColors { orbit_id, kind } => {
                write!(f, "orbit {orbit_id} ({kind}) does not hold 4 of each color")
            }
            Self::Frame => write!(f, "face centers are not a rotation of the solved frame"),
            Self::ParityLaw => write!(f, "corner, middle-edge and frame parities are inconsistent"),
        }
    }
}

impl std::error::Error for ValidationError {}

impl From<IdentityError> for ValidationError {
    fn from(e: IdentityError) -> Self {
        Self::Piece(e)
    }
}

/// Check that a color state is reachable with allowed moves: color counts, valid distinct
/// pieces, fixed corner, orientation sums, center color counts, frame, and the 3×3 parity law.
pub fn validate(cube: &Cube) -> Result<(), ValidationError> {
    let n = cube.n();
    let per_face = (n as usize).pow(2);
    let mut counts = [0usize; 6];
    for &c in cube.facelets() {
        counts[usize::from(c)] += 1;
    }
    for (color, &count) in counts.iter().enumerate() {
        if count != per_face {
            return Err(ValidationError::ColorCount {
                color: color as u8,
                count,
                expected: per_face,
            });
        }
    }
    let mut parity_law = false;
    for orbit in orbits(n) {
        let map = SlotMap::for_orbit(n, &orbit);
        let content = map.extract(cube)?;
        match orbit.kind {
            OrbitKind::Corner | OrbitKind::MidEdge | OrbitKind::Wing => {
                let w = if orbit.kind == OrbitKind::Wing {
                    1
                } else {
                    map.width as u8
                };
                let mut seen = vec![false; content.len()];
                for &x in &content {
                    let p = usize::from(x / w);
                    if std::mem::replace(&mut seen[p], true) {
                        return Err(ValidationError::DuplicatePiece {
                            orbit_id: orbit.id,
                            kind: orbit.kind,
                        });
                    }
                }
                let ori: u32 = content.iter().map(|&x| u32::from(x % w)).sum();
                match orbit.kind {
                    OrbitKind::Corner => {
                        if content[FIXED_CORNER_SLOT] != 3 * FIXED_CORNER_SLOT as u8 {
                            return Err(ValidationError::FixedCornerMoved);
                        }
                        if ori % 3 != 0 {
                            return Err(ValidationError::CornerTwist);
                        }
                        parity_law ^= piece_parity(orbit.kind, &content);
                    }
                    OrbitKind::MidEdge => {
                        if ori % 2 != 0 {
                            return Err(ValidationError::EdgeFlip);
                        }
                        parity_law ^= piece_parity(orbit.kind, &content);
                    }
                    _ => {}
                }
            }
            OrbitKind::FixedCenter => {
                let fi = frame_index(&content).ok_or(ValidationError::Frame)?;
                parity_law ^= frame_parity(fi);
            }
            _ => {
                let mut per_color = [0u8; 6];
                for &c in &content {
                    per_color[usize::from(c)] += 1;
                }
                if per_color != [4; 6] {
                    return Err(ValidationError::CenterColors {
                        orbit_id: orbit.id,
                        kind: orbit.kind,
                    });
                }
            }
        }
    }
    // Even N has no middle edges or frame, and its corner parity is free.
    if n % 2 == 1 && parity_law {
        return Err(ValidationError::ParityLaw);
    }
    Ok(())
}

/// Uniformly random reachable state, built orbit by orbit (CONVENTIONS §6).
///
/// RNG draw order: frame (odd N), corners, middle edges (odd N), then every other orbit in id
/// order. Changing this order changes which state a seed gives.
pub fn random_state(n: u32, seed: u64) -> Cube {
    let os = orbits(n);
    let mut r = rng(seed);
    let mut cube = Cube::solved(n);
    let find = |kind| os.iter().find(|o: &&Orbit| o.kind == kind);
    let map = |o: &Orbit| SlotMap::for_orbit(n, o);

    let mut frame_odd = false;
    if let Some(o) = find(OrbitKind::FixedCenter) {
        let fi = below(&mut r, 24) as usize;
        frame_odd = frame_parity(fi);
        map(o).insert(&mut cube, &frame_rotations()[fi]);
    }

    let corners = find(OrbitKind::Corner).expect("every cube has corners");
    let mut pieces: Vec<u8> = (0..8).filter(|&p| p != FIXED_CORNER_SLOT as u8).collect();
    shuffle(&mut r, &mut pieces);
    let mut twists: Vec<u8> = (0..6).map(|_| below(&mut r, 3) as u8).collect();
    twists.push((3 - twists.iter().map(|&t| u32::from(t)).sum::<u32>() % 3) as u8 % 3);
    let mut content = [0u8; 8];
    let movable = (0..8).filter(|&s| s != FIXED_CORNER_SLOT);
    for ((slot, piece), twist) in movable.zip(pieces).zip(twists) {
        content[slot] = 3 * piece + twist;
    }
    content[FIXED_CORNER_SLOT] = 3 * FIXED_CORNER_SLOT as u8;
    let corner_odd = piece_parity(OrbitKind::Corner, &content);
    map(corners).insert(&mut cube, &content);

    if let Some(o) = find(OrbitKind::MidEdge) {
        let mut pieces: Vec<u8> = (0..12).collect();
        shuffle(&mut r, &mut pieces);
        let as_content = |p: &[u8]| p.iter().map(|&x| 2 * x).collect::<Vec<_>>();
        if piece_parity(OrbitKind::MidEdge, &as_content(&pieces)) != (corner_odd ^ frame_odd) {
            pieces.swap(0, 1);
        }
        let mut flips: Vec<u8> = (0..11).map(|_| below(&mut r, 2) as u8).collect();
        flips.push(flips.iter().sum::<u8>() % 2);
        let content: Vec<u8> = pieces.iter().zip(flips).map(|(&p, f)| 2 * p + f).collect();
        map(o).insert(&mut cube, &content);
    }

    for o in &os {
        let content: Vec<u8> = match o.kind {
            OrbitKind::Corner | OrbitKind::MidEdge | OrbitKind::FixedCenter => continue,
            OrbitKind::Wing => {
                let mut p: Vec<u8> = (0..24).collect();
                shuffle(&mut r, &mut p);
                p
            }
            _ => {
                let mut colors: Vec<u8> = (0..24).map(|i| i / 4).collect();
                shuffle(&mut r, &mut colors);
                colors
            }
        };
        map(o).insert(&mut cube, &content);
    }
    cube
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slots::MID_EDGE_SLOTS;

    #[test]
    fn scrambles_and_random_states_validate() {
        for n in 2..=30 {
            for seed in 0..4 {
                let (c, _) = Cube::scramble(n, 120, seed);
                assert_eq!(validate(&c), Ok(()), "scramble n={n} seed={seed}");
                let rs = random_state(n, seed);
                assert_eq!(validate(&rs), Ok(()), "random_state n={n} seed={seed}");
                assert_eq!(rs, random_state(n, seed));
            }
        }
        assert_ne!(random_state(6, 1), random_state(6, 2));
    }

    fn orbit_map(n: u32, kind: OrbitKind) -> SlotMap {
        SlotMap::for_orbit(n, orbits(n).iter().find(|o| o.kind == kind).unwrap())
    }

    fn edit(n: u32, kind: OrbitKind, f: impl FnOnce(&mut Vec<u8>)) -> Cube {
        let mut c = random_state(n, 77);
        let map = orbit_map(n, kind);
        let mut content = map.extract(&c).unwrap();
        f(&mut content);
        map.insert(&mut c, &content);
        c
    }

    #[test]
    fn rejects_illegal_states() {
        // One corner twisted.
        let c = edit(5, OrbitKind::Corner, |v| {
            v[0] = v[0] / 3 * 3 + (v[0] % 3 + 1) % 3
        });
        assert_eq!(validate(&c), Err(ValidationError::CornerTwist));
        // One middle edge flipped.
        let c = edit(5, OrbitKind::MidEdge, |v| v[3] ^= 1);
        assert_eq!(validate(&c), Err(ValidationError::EdgeFlip));
        // Two middle edges swapped: breaks the 3×3 parity law.
        let c = edit(7, OrbitKind::MidEdge, |v| v.swap(0, 1));
        assert_eq!(validate(&c), Err(ValidationError::ParityLaw));
        // The fixed corner moved.
        let c = edit(4, OrbitKind::Corner, |v| v.swap(0, FIXED_CORNER_SLOT));
        assert_eq!(validate(&c), Err(ValidationError::FixedCornerMoved));
        // A duplicated wing: its twin (same colors, other hand) replaced by a second copy.
        let c = edit(6, OrbitKind::Wing, |v| v[1] ^= 1);
        assert!(matches!(
            validate(&c),
            Err(ValidationError::DuplicatePiece { .. })
        ));
        // A non-piece: a middle edge showing its colors on the wrong faces is fine, but U/U is not.
        let mut c = Cube::solved(3);
        let me = orbit_map(3, OrbitKind::MidEdge);
        c.set_stickers(&[(me.slot(0)[1], MID_EDGE_SLOTS[0][0] as u8)]);
        assert!(validate(&c).is_err());
        // Wrong color count.
        let mut c = Cube::solved(4);
        c.set_stickers(&[(0, 1)]);
        assert!(matches!(
            validate(&c),
            Err(ValidationError::ColorCount { .. })
        ));
    }

    #[test]
    fn accepts_free_parities() {
        // Wing parity is free; even-N corner parity is free.
        let c = edit(6, OrbitKind::Wing, |v| v.swap(0, 1));
        assert_eq!(validate(&c), Ok(()));
        let c = edit(6, OrbitKind::Corner, |v| v.swap(0, 1));
        assert_eq!(validate(&c), Ok(()));
    }
}
