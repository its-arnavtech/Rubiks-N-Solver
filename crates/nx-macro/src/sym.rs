//! Symbolic moves (ARCHITECTURE §6.1, CONVENTIONS §7): a move names a layer *reference*
//! that is bound to a concrete layer per orbit instance.

use std::fmt;
use std::str::FromStr;

use nx_sim::{Axis, Move, Orbit, OrbitKind, Turn};

/// Layer reference. `A_BAR = N−1−a`, `B_BAR = N−1−b`, `MID = (N−1)/2` (odd N).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LayerRef {
    Outer,
    Mid,
    A,
    ABar,
    B,
    BBar,
}

impl LayerRef {
    pub const ALL: [LayerRef; 6] = [
        LayerRef::Outer,
        LayerRef::Mid,
        LayerRef::A,
        LayerRef::ABar,
        LayerRef::B,
        LayerRef::BBar,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            LayerRef::Outer => "OUTER",
            LayerRef::Mid => "MID",
            LayerRef::A => "A",
            LayerRef::ABar => "A_BAR",
            LayerRef::B => "B",
            LayerRef::BBar => "B_BAR",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.name() == s)
    }

    /// The local generator refs of an orbit type (ARCHITECTURE §6.1). `Corner` uses `OUTER`
    /// only, so its macros exist at every N.
    pub fn for_kind(kind: OrbitKind) -> &'static [LayerRef] {
        use LayerRef::*;
        match kind {
            OrbitKind::Corner => &[Outer],
            OrbitKind::MidEdge => &[Outer, Mid],
            OrbitKind::Wing | OrbitKind::XCenter => &[Outer, A, ABar],
            OrbitKind::PlusCenter => &[Outer, A, ABar, Mid],
            OrbitKind::ObliqueA | OrbitKind::ObliqueB => &[Outer, A, ABar, B, BBar],
            OrbitKind::FixedCenter => &[Mid],
        }
    }
}

/// Concrete indices an orbit instance binds its layer refs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub n: u32,
    pub a: u32,
    pub b: u32,
}

impl Binding {
    pub fn new(n: u32, a: u32, b: u32) -> Self {
        Self { n, a, b }
    }

    pub fn for_orbit(n: u32, orbit: &Orbit) -> Self {
        Self::new(n, orbit.a, orbit.b)
    }

    pub fn layer(self, r: LayerRef) -> u32 {
        match r {
            LayerRef::Outer => 0,
            LayerRef::Mid => {
                debug_assert!(self.n % 2 == 1, "MID needs odd N");
                (self.n - 1) / 2
            }
            LayerRef::A => self.a,
            LayerRef::ABar => self.n - 1 - self.a,
            LayerRef::B => self.b,
            LayerRef::BBar => self.n - 1 - self.b,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymMove {
    pub axis: Axis,
    pub layer: LayerRef,
    pub turns: u8,
}

impl SymMove {
    pub const fn new(axis: Axis, layer: LayerRef, turns: u8) -> Self {
        Self { axis, layer, turns }
    }

    pub const fn inverse(self) -> Self {
        Self {
            turns: (4 - self.turns % 4) % 4,
            ..self
        }
    }

    pub fn bind(self, b: Binding) -> Move {
        Move::new(self.axis, b.layer(self.layer), self.turns)
    }

    /// All generators over `refs`: axes x, y, z × refs × turns 1..3, in that nesting order.
    pub fn generators(refs: &[LayerRef]) -> Vec<SymMove> {
        let mut out = Vec::with_capacity(9 * refs.len());
        for axis in Axis::ALL {
            for &r in refs {
                for t in 1..=3 {
                    out.push(SymMove::new(axis, r, t));
                }
            }
        }
        out
    }
}

impl Turn for SymMove {
    fn axis(self) -> Axis {
        self.axis
    }
    fn layer_key(self) -> u32 {
        self.layer as u32
    }
    fn turns(self) -> u8 {
        self.turns
    }
    fn with_turns(self, turns: u8) -> Self {
        Self { turns, ..self }
    }
}

impl fmt::Display for SymMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            self.axis.name(),
            self.layer.name(),
            self.turns
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseSymError(pub String);

impl fmt::Display for ParseSymError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid symbolic move {:?}", self.0)
    }
}

impl std::error::Error for ParseSymError {}

impl FromStr for SymMove {
    type Err = ParseSymError;

    /// `"<axis>:<ref>:<turns>"`, e.g. `"x:A_BAR:3"`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseSymError(s.to_string());
        let mut parts = s.split(':');
        let axis = match parts.next() {
            Some("x") => Axis::X,
            Some("y") => Axis::Y,
            Some("z") => Axis::Z,
            _ => return Err(err()),
        };
        let layer = parts.next().and_then(LayerRef::from_name).ok_or_else(err)?;
        let turns = match parts.next() {
            Some("1") => 1,
            Some("2") => 2,
            Some("3") => 3,
            _ => return Err(err()),
        };
        if parts.next().is_some() {
            return Err(err());
        }
        Ok(Self::new(axis, layer, turns))
    }
}

/// Inverse of a symbolic sequence.
pub fn invert(seq: &[SymMove]) -> Vec<SymMove> {
    seq.iter().rev().map(|m| m.inverse()).collect()
}

/// Bind a symbolic sequence to concrete moves.
pub fn instantiate(seq: &[SymMove], b: Binding) -> Vec<Move> {
    seq.iter().map(|m| m.bind(b)).collect()
}

/// `S · M · S⁻¹`, cancelled.
pub fn conjugate(setup: &[SymMove], body: &[SymMove]) -> Vec<SymMove> {
    let mut seq = setup.to_vec();
    seq.extend_from_slice(body);
    seq.extend(invert(setup));
    nx_sim::cancel(&seq)
}

pub fn format_seq(seq: &[SymMove]) -> Vec<String> {
    seq.iter().map(SymMove::to_string).collect()
}

pub fn parse_seq<S: AsRef<str>>(items: &[S]) -> Result<Vec<SymMove>, ParseSymError> {
    items.iter().map(|s| s.as_ref().parse()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nx_sim::{Cube, orbits};

    #[test]
    fn string_round_trip() {
        for r in LayerRef::ALL {
            for axis in Axis::ALL {
                for t in 1..=3 {
                    let m = SymMove::new(axis, r, t);
                    assert_eq!(m.to_string().parse::<SymMove>(), Ok(m));
                }
            }
        }
        assert_eq!(
            SymMove::new(Axis::Y, LayerRef::ABar, 3).to_string(),
            "y:A_BAR:3"
        );
        for bad in ["", "x:A", "w:A:1", "x:C:1", "x:A:0", "x:A:4", "x:A:1:2"] {
            assert!(bad.parse::<SymMove>().is_err(), "{bad}");
        }
    }

    #[test]
    fn binding_and_inverse() {
        let b = Binding::new(9, 2, 3);
        let layers: Vec<u32> = LayerRef::ALL.iter().map(|&r| b.layer(r)).collect();
        assert_eq!(layers, vec![0, 4, 2, 6, 3, 5]);
        let seq = parse_seq(&["x:A:1", "y:OUTER:2", "z:B_BAR:3"]).unwrap();
        let mut c = Cube::solved(9);
        c.apply_all(&instantiate(&seq, b));
        c.apply_all(&instantiate(&invert(&seq), b));
        assert!(c.is_solved());
        assert_eq!(
            format_seq(&conjugate(&seq[..1], &seq[..1])),
            vec!["x:A:1".to_string()]
        );
    }

    #[test]
    fn refs_bind_to_distinct_allowed_layers_for_every_instance() {
        for n in 2..=25 {
            for o in orbits(n) {
                let b = Binding::for_orbit(n, &o);
                let refs = LayerRef::for_kind(o.kind);
                let layers: Vec<u32> = refs.iter().map(|&r| b.layer(r)).collect();
                let mut uniq = layers.clone();
                uniq.sort_unstable();
                uniq.dedup();
                assert_eq!(uniq.len(), layers.len(), "n={n} {:?}", o.kind);
                assert!(layers.iter().all(|&l| l + 2 <= n), "n={n} {:?}", o.kind);
            }
        }
    }
}
