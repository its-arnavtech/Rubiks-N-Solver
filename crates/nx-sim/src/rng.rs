//! Seeded randomness (CONVENTIONS §6, §9): `ChaCha8Rng` from a `u64` seed, and bounded
//! sampling implemented here so the streams never depend on `rand`'s sampling algorithms.

pub use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

/// The project's RNG for a seed. Identical on every OS.
pub fn rng(seed: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed)
}

/// Uniform integer in `0..bound` by rejection sampling. `bound` must be non-zero.
pub fn below(rng: &mut impl RngCore, bound: u64) -> u64 {
    assert!(bound > 0, "empty range");
    // Accept v < 2^64 − (2^64 mod bound), a multiple of bound.
    let rem = (u64::MAX % bound + 1) % bound;
    loop {
        let v = rng.next_u64();
        if rem == 0 || v < 0u64.wrapping_sub(rem) {
            return v % bound;
        }
    }
}

/// Uniform integer in `lo..=hi`.
pub fn between(rng: &mut impl RngCore, lo: u64, hi: u64) -> u64 {
    assert!(lo <= hi, "empty range");
    lo + below(rng, hi - lo + 1)
}

/// Fisher–Yates shuffle driven by [`below`].
pub fn shuffle<T>(rng: &mut impl RngCore, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = below(rng, i as u64 + 1) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_in_range() {
        let a: Vec<u64> = (0..100).scan(rng(7), |r, _| Some(below(r, 10))).collect();
        let b: Vec<u64> = (0..100).scan(rng(7), |r, _| Some(below(r, 10))).collect();
        assert_eq!(a, b);
        assert!(a.iter().all(|&v| v < 10));
        assert!((0..10).all(|d| a.contains(&d)));
        let mut r = rng(1);
        assert!((0..1000).all(|_| (3..=5).contains(&between(&mut r, 3, 5))));
        assert_eq!(below(&mut r, 1), 0);
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut v: Vec<u32> = (0..50).collect();
        shuffle(&mut rng(3), &mut v);
        let mut s = v.clone();
        s.sort_unstable();
        assert_eq!(s, (0..50).collect::<Vec<_>>());
        assert_ne!(v, s);
    }
}
