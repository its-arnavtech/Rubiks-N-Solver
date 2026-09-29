//! Slot effects: what a move sequence does to the 24 (8, 12) canonical slots of one orbit.
//!
//! Gather semantics (CONVENTIONS §7): `new[i] = old[perm[i]]`, then the piece's orientation
//! gains `ori[i]` mod the type's orientation modulus.

use nx_sim::{OrbitKind, SlotMap};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Effect {
    pub perm: Vec<u8>,
    pub ori: Vec<u8>,
}

/// Orientation modulus of a type: 3 for corners, 2 for middle edges, 1 otherwise.
pub const fn orientation_mod(kind: OrbitKind) -> u8 {
    match kind {
        OrbitKind::Corner => 3,
        OrbitKind::MidEdge => 2,
        _ => 1,
    }
}

impl Effect {
    pub fn identity(slots: usize) -> Self {
        Self {
            perm: (0..slots as u8).collect(),
            ori: vec![0; slots],
        }
    }

    pub fn is_identity(&self) -> bool {
        self.perm
            .iter()
            .enumerate()
            .all(|(i, &p)| usize::from(p) == i)
            && self.ori.iter().all(|&o| o == 0)
    }

    /// `self` followed by `next`.
    pub fn then(&self, next: &Effect, m: u8) -> Effect {
        let perm = next
            .perm
            .iter()
            .map(|&p| self.perm[usize::from(p)])
            .collect();
        let ori = next
            .perm
            .iter()
            .zip(&next.ori)
            .map(|(&p, &o)| (self.ori[usize::from(p)] + o) % m)
            .collect();
        Effect { perm, ori }
    }

    pub fn inverse(&self, m: u8) -> Effect {
        let len = self.perm.len();
        let mut perm = vec![0u8; len];
        let mut ori = vec![0u8; len];
        for (i, (&p, &o)) in self.perm.iter().zip(&self.ori).enumerate() {
            // The piece at slot p moves to slot i gaining o; the inverse moves it back.
            perm[usize::from(p)] = i as u8;
            ori[usize::from(p)] = (m - o % m) % m;
        }
        Effect { perm, ori }
    }

    /// Apply to slot contents (`piece·m + ori`, or piece / color when `m == 1`).
    pub fn apply(&self, content: &[u8], m: u8) -> Vec<u8> {
        self.perm
            .iter()
            .zip(&self.ori)
            .map(|(&p, &o)| {
                let x = content[usize::from(p)];
                if m == 1 {
                    x
                } else {
                    x / m * m + (x % m + o) % m
                }
            })
            .collect()
    }

    /// Classify as a macro kind: a 3-cycle (orientation changes only on the three cycled
    /// slots), or a pair of in-place orientation changes (corner twist pair, edge flip pair).
    pub fn classify(&self) -> Option<(MacroKind, Vec<u8>)> {
        let moved: Vec<u8> = (0..self.perm.len() as u8)
            .filter(|&i| self.perm[usize::from(i)] != i)
            .collect();
        let turned: Vec<u8> = (0..self.perm.len() as u8)
            .filter(|&i| self.perm[usize::from(i)] == i && self.ori[usize::from(i)] != 0)
            .collect();
        match (moved.len(), turned.len()) {
            (3, 0) => {
                // Cycle order: a → b → c, where the piece at a moves to b.
                let a = moved[0];
                let b = self.perm.iter().position(|&p| p == a)? as u8;
                let c = self.perm.iter().position(|&p| p == b)? as u8;
                (self.perm[usize::from(a)] == c).then(|| (MacroKind::ThreeCycle, vec![a, b, c]))
            }
            (0, 2) => Some((MacroKind::OrientPair, turned)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MacroKind {
    ThreeCycle,
    /// Corner twist pair or middle-edge flip pair.
    OrientPair,
}

impl MacroKind {
    pub fn name(self, kind: OrbitKind) -> &'static str {
        match (self, kind) {
            (MacroKind::ThreeCycle, _) => "three_cycle",
            (MacroKind::OrientPair, OrbitKind::Corner) => "twist_pair",
            (MacroKind::OrientPair, _) => "flip_pair",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "three_cycle" => Some(MacroKind::ThreeCycle),
            "twist_pair" | "flip_pair" => Some(MacroKind::OrientPair),
            _ => None,
        }
    }
}

/// Reads slot effects of one orbit instance from labeled facelet arrays.
#[derive(Clone, Debug)]
pub struct OrbitProbe {
    pub map: SlotMap,
    /// For each sticker: `slot · width + position` if it belongs to the orbit, else `u32::MAX`.
    home: Vec<u32>,
    m: u8,
}

impl OrbitProbe {
    pub fn new(map: SlotMap, sticker_count: usize) -> Self {
        let mut home = vec![u32::MAX; sticker_count];
        for (k, &s) in map.stickers.iter().enumerate() {
            home[s as usize] = k as u32;
        }
        let m = orientation_mod(map.kind);
        Self { map, home, m }
    }

    pub fn orientation_mod(&self) -> u8 {
        self.m
    }

    pub fn contains(&self, sticker: usize) -> bool {
        self.home[sticker] != u32::MAX
    }

    /// Effect on this orbit of a labeled state (`facelets[j]` = home sticker at position j),
    /// or `None` if a slot holds a sticker from elsewhere or a piece is split.
    pub fn effect(&self, facelets: &[u32]) -> Option<Effect> {
        let w = self.map.width;
        let slots = self.map.slot_count();
        let mut perm = Vec::with_capacity(slots);
        let mut ori = Vec::with_capacity(slots);
        for i in 0..slots {
            let st = self.map.slot(i);
            let homes: Vec<u32> = st
                .iter()
                .map(|&s| self.home[facelets[s as usize] as usize])
                .collect();
            if homes.contains(&u32::MAX) {
                return None;
            }
            let piece = homes[0] as usize / w;
            if homes.iter().any(|&h| h as usize / w != piece) {
                return None;
            }
            let o = if self.m == 1 {
                0
            } else {
                homes.iter().position(|&h| h as usize % w == 0)? as u8
            };
            perm.push(piece as u8);
            ori.push(o);
        }
        Some(Effect { perm, ori })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(perm: &[u8], ori: &[u8]) -> Effect {
        Effect {
            perm: perm.to_vec(),
            ori: ori.to_vec(),
        }
    }

    #[test]
    fn compose_inverse_apply() {
        let a = e(&[1, 2, 0, 3], &[1, 2, 0, 0]);
        let b = e(&[0, 3, 2, 1], &[0, 1, 1, 2]);
        let content = [0u8, 3, 6, 9]; // pieces 0..3, orientation 0, m = 3
        let ab = a.then(&b, 3);
        assert_eq!(ab.apply(&content, 3), b.apply(&a.apply(&content, 3), 3));
        assert!(a.then(&a.inverse(3), 3).is_identity());
        assert!(a.inverse(3).then(&a, 3).is_identity());
    }

    #[test]
    fn classify_kinds() {
        let c = e(&[2, 1, 3, 0, 4], &[0; 5]);
        assert_eq!(c.classify(), Some((MacroKind::ThreeCycle, vec![0, 3, 2])));
        let t = e(&[0, 1, 2, 3], &[1, 0, 2, 0]);
        assert_eq!(t.classify(), Some((MacroKind::OrientPair, vec![0, 2])));
        assert_eq!(e(&[1, 0, 2], &[0; 3]).classify(), None);
        assert_eq!(e(&[2, 1, 3, 0], &[0, 1, 0, 0]).classify(), None);
    }
}
