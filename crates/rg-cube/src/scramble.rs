use crate::moves::{Move, generator_moves};

/// Small, fast, seedable PRNG (SplitMix64). Deterministic across native and wasm.
#[derive(Clone, Debug)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in `0..bound` (Lemire's multiply-shift; bias is negligible here).
    pub fn below(&mut self, bound: u64) -> u64 {
        ((u128::from(self.next_u64()) * u128::from(bound)) >> 64) as u64
    }
}

/// Default random-move scramble length per size.
pub fn scramble_length(n: u8) -> usize {
    match n {
        2 => 11,
        3 => 25,
        4 => 40,
        _ => 60,
    }
}

/// Random canonical move sequence from the puzzle's move set: consecutive turns on
/// the same axis must be in strictly increasing layer-mask order (docs 02 §10).
pub fn random_move_scramble(n: u8, len: usize, seed: u64) -> Vec<Move> {
    let gens = generator_moves(n);
    let mut rng = SplitMix64::new(seed);
    let mut out: Vec<Move> = Vec::with_capacity(len);
    while out.len() < len {
        let g = gens[rng.below(gens.len() as u64) as usize];
        if let Some(prev) = out.last() {
            if prev.axis == g.axis && g.layers <= prev.layers {
                continue;
            }
        }
        let turns = 1 + rng.below(3) as u8;
        out.push(Move { turns, ..g });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrambles_are_deterministic_and_canonical() {
        let a = random_move_scramble(4, 40, 99);
        assert_eq!(a, random_move_scramble(4, 40, 99));
        assert_ne!(a, random_move_scramble(4, 40, 100));
        for w in a.windows(2) {
            assert!(w[0].axis != w[1].axis || w[0].layers < w[1].layers);
        }
    }
}
