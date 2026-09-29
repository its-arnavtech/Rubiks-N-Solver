//! Ranking and unranking of the combinatorial objects that coordinates index
//! (docs/06 §3).

const PASCAL_N: usize = 33;

/// Pascal's triangle, built at compile time so ranking stays branch-light in hot loops.
static PASCAL: [[u64; PASCAL_N]; PASCAL_N] = {
    let mut t = [[0u64; PASCAL_N]; PASCAL_N];
    let mut n = 0;
    while n < PASCAL_N {
        t[n][0] = 1;
        let mut k = 1;
        while k <= n {
            t[n][k] = t[n - 1][k - 1] + if k < n { t[n - 1][k] } else { 0 };
            k += 1;
        }
        n += 1;
    }
    t
};

/// Binomial coefficient C(n, k); 0 when k > n.
pub fn binomial(n: u32, k: u32) -> u64 {
    if k > n {
        return 0;
    }
    if (n as usize) < PASCAL_N {
        return PASCAL[n as usize][k as usize];
    }
    let k = k.min(n - k);
    (0..k).fold(1u64, |acc, i| acc * u64::from(n - i) / u64::from(i + 1))
}

/// Rank of a k-subset given as a bit mask (same numbering as [`comb_rank`]).
pub fn mask_rank(mask: u32) -> u32 {
    let (mut rank, mut i, mut m) = (0u64, 1u32, mask);
    while m != 0 {
        rank += binomial(m.trailing_zeros(), i);
        i += 1;
        m &= m - 1;
    }
    rank as u32
}

/// Inverse of [`mask_rank`] for a k-subset of `0..n`.
pub fn mask_unrank(rank: u32, n: u32, k: usize) -> u32 {
    let mut r = u64::from(rank);
    let mut mask = 0u32;
    let mut c = n;
    for i in (0..k).rev() {
        c -= 1;
        while binomial(c, i as u32 + 1) > r {
            c -= 1;
        }
        mask |= 1 << c;
        r -= binomial(c, i as u32 + 1);
    }
    mask
}

/// Lehmer-code rank of a permutation of `0..p.len()`. The identity has rank 0.
pub fn perm_rank(p: &[u8]) -> u32 {
    let n = p.len();
    let mut rank = 0u32;
    for i in 0..n {
        let smaller = p[i + 1..].iter().filter(|&&x| x < p[i]).count() as u32;
        rank = rank * (n - i) as u32 + smaller;
    }
    rank
}

/// Inverse of [`perm_rank`].
pub fn perm_unrank(mut rank: u32, n: usize) -> Vec<u8> {
    let mut digits = vec![0u32; n];
    for (i, d) in digits.iter_mut().enumerate().rev() {
        let base = (n - i) as u32;
        *d = rank % base;
        rank /= base;
    }
    let mut available: Vec<u8> = (0..n as u8).collect();
    digits
        .iter()
        .map(|&d| available.remove(d as usize))
        .collect()
}

/// Rank of a k-subset of `0..n`, given in ascending order (combinatorial number system).
pub fn comb_rank(sorted: &[u8]) -> u32 {
    sorted
        .iter()
        .enumerate()
        .map(|(i, &c)| binomial(u32::from(c), i as u32 + 1))
        .sum::<u64>() as u32
}

/// Inverse of [`comb_rank`]: the ascending k-subset of `0..n` with this rank.
pub fn comb_unrank(rank: u32, n: u32, k: usize) -> Vec<u8> {
    let mut out = vec![0u8; k];
    let mut r = u64::from(rank);
    let mut c = n;
    for i in (0..k).rev() {
        c -= 1;
        while binomial(c, i as u32 + 1) > r {
            c -= 1;
        }
        out[i] = c as u8;
        r -= binomial(c, i as u32 + 1);
    }
    out
}

/// True if the permutation is odd.
pub fn is_odd(p: &[u8]) -> bool {
    let inversions: usize = (0..p.len())
        .map(|i| p[i + 1..].iter().filter(|&&x| x < p[i]).count())
        .sum();
    inversions % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn binomials() {
        assert_eq!(binomial(12, 4), 495);
        assert_eq!(binomial(24, 8), 735_471);
        assert_eq!(binomial(24, 12), 2_704_156);
        assert_eq!(binomial(3, 5), 0);
    }

    #[test]
    fn permutations_round_trip_exhaustively() {
        let mut seen = HashSet::new();
        for r in 0..5040 {
            let p = perm_unrank(r, 7);
            assert_eq!(perm_rank(&p), r);
            assert!(seen.insert(p));
        }
        assert_eq!(perm_rank(&[0, 1, 2, 3]), 0);
    }

    #[test]
    fn combinations_round_trip_exhaustively() {
        let mut seen = HashSet::new();
        for r in 0..495 {
            let c = comb_unrank(r, 12, 4);
            assert!(c.windows(2).all(|w| w[0] < w[1]) && c[3] < 12);
            assert_eq!(comb_rank(&c), r);
            assert!(seen.insert(c));
        }
    }

    #[test]
    fn masks_round_trip_and_agree_with_slices() {
        for r in 0..495 {
            let mask = mask_unrank(r, 12, 4);
            assert_eq!(mask.count_ones(), 4);
            assert_eq!(mask_rank(mask), r);
            let sorted: Vec<u8> = (0..12u8).filter(|&b| mask >> b & 1 == 1).collect();
            assert_eq!(comb_rank(&sorted), r);
        }
        // The 4x4 centre coordinate: 8 of 24 slots.
        assert_eq!(binomial(24, 8), 735_471);
        for r in (0..735_471).step_by(9973) {
            assert_eq!(mask_rank(mask_unrank(r, 24, 8)), r);
        }
    }

    #[test]
    fn parity() {
        assert!(!is_odd(&[0, 1, 2]));
        assert!(is_odd(&[1, 0, 2]));
        assert!(!is_odd(&[1, 2, 0]));
    }
}
