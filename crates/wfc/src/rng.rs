//! Vendored xoshiro256** PRNG.
//!
//! Vendored rather than taken from `rand` because generated maps are content:
//! a dependency bump that changed stream output would silently invalidate every
//! recorded seed and every golden test in the workspace.

/// xoshiro256** — 256-bit state, 2^256 period, deterministic from a `u64` seed.
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Seeds all four words from `seed` via SplitMix64, per the reference
    /// implementation's recommendation (a raw seed with mostly-zero words
    /// produces poor initial output).
    pub fn new(seed: u64) -> Self {
        let mut z = seed;
        let mut s = [0u64; 4];
        for word in &mut s {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            *word = x ^ (x >> 31);
        }
        Rng { s }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in `[0, 1)`, using the top 53 bits (f64 mantissa width).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    /// Pins the stream so a refactor cannot silently change generated content.
    #[test]
    fn stream_is_stable_for_a_seed() {
        let mut rng = Rng::new(42);
        let got: Vec<u64> = (0..4).map(|_| rng.next_u64()).collect();
        assert_eq!(
            got,
            vec![
                1546998764402558742,
                6990951692964543102,
                12544586762248559009,
                17057574109182124193
            ]
        );
    }

    #[test]
    fn distinct_seeds_give_distinct_streams() {
        let a = Rng::new(1).next_u64();
        let b = Rng::new(2).next_u64();
        assert_ne!(a, b);
    }

    #[test]
    fn floats_stay_in_unit_interval() {
        let mut rng = Rng::new(7);
        for _ in 0..1000 {
            let f = rng.next_f64();
            assert!((0.0..1.0).contains(&f), "out of range: {f}");
        }
    }
}
