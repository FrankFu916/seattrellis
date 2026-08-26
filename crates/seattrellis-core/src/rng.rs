//! Deterministic PRNGs used by the cost module.
//!
//! Python's `random.Random` is stateful and non-reproducible across runtimes, so
//! the port uses a fixed-seed deterministic generator. Two are provided:
//!
//! * [`SplitMix64`] — the primary generator (small, fast, seed=0 default). It is
//!   used wherever the module needs randomness and is the drop-in replacement
//!   for `random.Random` mandated by the port.
//! * [`PythonMt`] — an MT19937 that byte-for-byte replicates CPython's
//!   `random.Random` (`randint`/`randbelow`/`getrandbits` semantics) so the
//!   port can be cross-checked against *real* Python output with an identical
//!   seed. It is only used by the parity tests, not by the module's public API.
//!
//! The common trait [`RandInt`] lets [`individual_cost`](crate::cost::individual_cost)
//! drive either generator.

/// Anything that can produce an inclusive-bounds `randint(lo, hi)`, mirroring
/// `random.Random.randint`.
pub trait RandInt {
    /// Return a random integer `n` with `lo <= n <= hi`.
    fn randint(&mut self, lo: i32, hi: i32) -> i32;
}

/// SplitMix64 — a tiny, well-studied deterministic PRNG.
///
/// Default seed 0 is used for reproducible, fixed-seed behavior.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const MIX1: u64 = 0xBF58_476D_1CE4_E5B9;
const MIX2: u64 = 0x94D0_49BB_1331_11EB;

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(MIX1);
        z = (z ^ (z >> 27)).wrapping_mul(MIX2);
        z ^ (z >> 31)
    }

    /// Uniform index in `0..bound` (the solver's choice helper).
    pub fn next_usize(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            return 0;
        }
        (self.next_u64() % bound as u64) as usize
    }
}

impl RandInt for SplitMix64 {
    fn randint(&mut self, lo: i32, hi: i32) -> i32 {
        // Widen before subtracting: `hi - lo + 1` overflows i32 when the
        // bounds span the full range (e.g. lo = i32::MIN, hi = i32::MAX).
        let range = i64::from(hi) - i64::from(lo) + 1;
        (i64::from(lo) + (self.next_u64() % range as u64) as i64) as i32
    }
}

/// MT19937 replicating CPython's `random.Random` for exact cross-language parity.
///
/// Seed handling mirrors `_randommodule.c`: an integer seed is fed through
/// `init_by_array` with a single 32-bit word, exactly as `random.Random(n)` does.
/// `randint(lo, hi)` mirrors `randrange` + `_randbelow` + `getrandbits`, including
/// the rejection sampling and the power-of-two shortcut.
#[derive(Debug, Clone)]
pub struct PythonMt {
    mt: [u32; 624],
    index: usize,
}

const MT_N: usize = 624;
const MT_M: usize = 397;
const MATRIX_A: u32 = 0x9908_B0DF;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7FFF_FFFF;

impl PythonMt {
    pub fn new(seed: u32) -> Self {
        let mut mt = [0u32; MT_N];
        // init_genrand(19650218U) — the prefix used by init_by_array.
        mt[0] = 19_650_218;
        for i in 1..MT_N {
            mt[i] = 1812433253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }

        // init_by_array with a single 32-bit key [seed], matching
        // random.Random(seed) for integer seeds.
        let key = [seed];
        let mut i = 1usize;
        let mut j = 0usize;
        let mut k = MT_N.max(key.len());
        while k > 0 {
            // mt[i] = (mt[i] ^ ((mt[i-1] ^ (mt[i-1] >> 30)) * 1664525)) + key[j] + j
            // The `+ key[j] + j` applies *after* the XOR.
            let x = mt[i - 1] ^ (mt[i - 1] >> 30);
            mt[i] = (mt[i] ^ x.wrapping_mul(1664525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= MT_N {
                mt[0] = mt[MT_N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
            k -= 1;
        }
        let mut k = MT_N - 1;
        while k > 0 {
            // mt[i] = (mt[i] ^ ((mt[i-1] ^ (mt[i-1] >> 30)) * 1566083941)) - i
            let x = mt[i - 1] ^ (mt[i - 1] >> 30);
            mt[i] = (mt[i] ^ x.wrapping_mul(1566083941)).wrapping_sub(i as u32);
            i += 1;
            if i >= MT_N {
                mt[0] = mt[MT_N - 1];
                i = 1;
            }
            k -= 1;
        }
        mt[0] = 0x8000_0000; // MSB is 1; assures non-zero initial array

        Self { mt, index: MT_N }
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= MT_N {
            let mut y: u32;
            let mut kk = 0usize;
            while kk < MT_N - MT_M {
                y = (self.mt[kk] & UPPER_MASK) | (self.mt[kk + 1] & LOWER_MASK);
                self.mt[kk] = self.mt[kk + MT_M] ^ (y >> 1) ^ (y & 1).wrapping_mul(MATRIX_A);
                kk += 1;
            }
            while kk < MT_N - 1 {
                y = (self.mt[kk] & UPPER_MASK) | (self.mt[kk + 1] & LOWER_MASK);
                // (kk + MT_M) - MT_N == kk - (MT_N - MT_M); kk >= MT_N - MT_M here so
                // this does not underflow.
                self.mt[kk] = self.mt[kk + MT_M - MT_N] ^ (y >> 1) ^ (y & 1).wrapping_mul(MATRIX_A);
                kk += 1;
            }
            y = (self.mt[MT_N - 1] & UPPER_MASK) | (self.mt[0] & LOWER_MASK);
            self.mt[MT_N - 1] = self.mt[MT_M - 1] ^ (y >> 1) ^ (y & 1).wrapping_mul(MATRIX_A);
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9D2C_5680;
        y ^= (y << 15) & 0xEFC6_0000;
        y ^= y >> 18;
        y
    }

    /// `getrandbits(k)` for `1 <= k <= 32`, matching CPython.
    fn getrandbits(&mut self, k: u32) -> u32 {
        self.next_u32() >> (32 - k)
    }

    /// `_randbelow(n)` for `1 <= n <= 2^32 - 1`, matching CPython.
    fn randbelow(&mut self, n: u32) -> u32 {
        debug_assert!(n >= 1);
        let bl = 32 - (n - 1).leading_zeros(); // bit_length(n)
        let k = if n & (n - 1) == 0 { bl - 1 } else { bl };
        loop {
            let r = self.getrandbits(k);
            if r < n {
                return r;
            }
        }
    }
}

impl RandInt for PythonMt {
    fn randint(&mut self, lo: i32, hi: i32) -> i32 {
        let n = (hi - lo + 1) as u32;
        lo + self.randbelow(n) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_randint_matches_reference() {
        let mut sm = SplitMix64::new(0);
        let got: Vec<i32> = (0..12).map(|_| sm.randint(0, 100)).collect();
        assert_eq!(got, vec![67, 26, 88, 12, 14, 87, 73, 81, 18, 89, 100, 32]);
    }

    #[test]
    fn splitmix64_randint_survives_full_i32_range() {
        // (hi - lo + 1) used to overflow i32 for the full span (gate L-2).
        let mut sm = SplitMix64::new(0);
        let values: Vec<i32> = (0..256).map(|_| sm.randint(i32::MIN, i32::MAX)).collect();
        assert!(values.iter().any(|&value| value < 0));
        assert!(values.iter().any(|&value| value >= 0));
        assert_eq!(sm.randint(-5, -5), -5);
        assert_eq!(sm.randint(i32::MIN, i32::MIN), i32::MIN);
        assert_eq!(sm.randint(i32::MAX, i32::MAX), i32::MAX);
    }

    #[test]
    fn python_mt_randint_matches_real_python() {
        // random.Random(0).randint(0, 100) first 12 values from the venv.
        let mut mt = PythonMt::new(0);
        let got: Vec<i32> = (0..12).map(|_| mt.randint(0, 100)).collect();
        assert_eq!(got, vec![49, 97, 53, 5, 33, 65, 62, 51, 100, 38, 61, 45]);
    }

    #[test]
    fn python_mt_seed_42_matches_real_python() {
        let mut mt = PythonMt::new(42);
        let got: Vec<i32> = (0..5).map(|_| mt.randint(0, 100)).collect();
        assert_eq!(got, vec![81, 14, 3, 94, 35]);
    }
}
