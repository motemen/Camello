//! The hasher the maps keyed on a file offset use.
//!
//! Both maps in the formatter's path — the trivia model's, and the one
//! memoising which blocks can stay flat — are keyed on a `TextSize`: a `u32`
//! offset into the file being formatted. SipHash, which `std` reaches for by
//! default, costs more on such a key than the lookup it guards, and it is
//! guarding against an attacker choosing keys to collide. The keys here come
//! from where a token happens to start in the file, so there is no such
//! attacker and nothing to buy with the cost.
//!
//! The mixing step is the one rustc uses for the same reason: rotate, xor in the
//! word, multiply by a constant with the bits spread out.

use std::hash::{BuildHasherDefault, Hasher};

pub type OffsetMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<OffsetHasher>>;

const MULTIPLIER: u64 = 0x517c_c1b7_2722_0a95;

#[derive(Default)]
pub struct OffsetHasher {
    hash: u64,
}

impl OffsetHasher {
    #[inline]
    fn mix(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(MULTIPLIER);
    }
}

impl Hasher for OffsetHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.mix(u64::from(*byte));
        }
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.mix(u64::from(value));
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.mix(value);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.mix(value as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}
