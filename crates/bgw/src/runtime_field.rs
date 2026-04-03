//! Runtime-configurable prime field for arbitrary primes < 2^63.
//!
//! Replaces the compile-time `Mersenne63 = Fp64<MontBackend<...>>` type so that
//! BGW arithmetic can work over any prime the circuit declares.

use rand::Rng;
use runtime::vm::BackendError;

/// A prime field with a runtime-chosen modulus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimeField {
    pub modulus: u64,
}

impl PrimeField {
    pub fn new(modulus: u64) -> Self {
        Self { modulus }
    }

    /// Default: Mersenne prime 2^63 - 1.
    pub fn mersenne63() -> Self {
        Self { modulus: (1u64 << 63) - 1 }
    }

    #[inline]
    pub fn add(&self, a: u64, b: u64) -> u64 {
        // a, b < modulus ≤ 2^63-1, so a+b < 2^64 — no overflow with u64.
        let s = a + b;
        if s >= self.modulus { s - self.modulus } else { s }
    }

    #[inline]
    pub fn sub(&self, a: u64, b: u64) -> u64 {
        if a >= b { a - b } else { self.modulus - (b - a) }
    }

    #[inline]
    pub fn mul(&self, a: u64, b: u64) -> u64 {
        ((a as u128 * b as u128) % self.modulus as u128) as u64
    }

    #[inline]
    pub fn neg(&self, a: u64) -> u64 {
        if a == 0 { 0 } else { self.modulus - a }
    }

    /// Modular inverse via extended Euclidean. Returns `None` if `a == 0`.
    pub fn inv(&self, a: u64) -> Option<u64> {
        if a == 0 {
            return None;
        }
        // Extended Euclidean algorithm (Bezout).
        let m = self.modulus as i128;
        let mut r0 = m;
        let mut r1 = a as i128;
        let mut s0: i128 = 0;
        let mut s1: i128 = 1;
        while r1 != 0 {
            let q = r0 / r1;
            let r2 = r0 - q * r1;
            let s2 = s0 - q * s1;
            r0 = r1; r1 = r2;
            s0 = s1; s1 = s2;
        }
        // r0 == gcd; if gcd != 1 the element has no inverse in this ring.
        if r0 != 1 {
            return None;
        }
        // s0 is the Bezout coefficient; reduce to [0, modulus).
        let inv = ((s0 % m) + m) % m;
        Some(inv as u64)
    }

    /// Reduce a `u64` value into the field.
    #[inline]
    pub fn from_u64(&self, v: u64) -> u64 {
        v % self.modulus
    }

    /// Random field element in `[0, modulus)`.
    pub fn rand<R: Rng + ?Sized>(&self, rng: &mut R) -> u64 {
        rng.gen_range(0..self.modulus)
    }

    /// Serialise a field element as 8 little-endian bytes.
    pub fn to_bytes(v: u64) -> [u8; 8] {
        v.to_le_bytes()
    }

    /// Deserialise 8 little-endian bytes into a raw `u64`.
    /// The caller is responsible for ensuring `value < modulus` if needed.
    pub fn from_bytes(bytes: &[u8]) -> Result<u64, BackendError> {
        if bytes.len() < 8 {
            return Err(BackendError::BackendError(format!(
                "expected 8 bytes for field element, got {}",
                bytes.len()
            )));
        }
        Ok(u64::from_le_bytes(bytes[..8].try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_wraps_correctly() {
        let f = PrimeField::new(7);
        assert_eq!(f.add(5, 4), 2); // 9 % 7 = 2
        assert_eq!(f.add(0, 0), 0);
        assert_eq!(f.add(6, 1), 0); // 7 % 7 = 0
    }

    #[test]
    fn sub_wraps_correctly() {
        let f = PrimeField::new(7);
        assert_eq!(f.sub(3, 5), 5); // 3 - 5 = -2 ≡ 5 (mod 7)
        assert_eq!(f.sub(5, 3), 2);
    }

    #[test]
    fn mul_uses_u128() {
        let f = PrimeField::new(7);
        assert_eq!(f.mul(3, 5), 1); // 15 % 7 = 1
        // Large product that overflows u64 if naive
        let mersenne = PrimeField::mersenne63();
        let big = mersenne.modulus - 1;
        assert_eq!(mersenne.mul(big, big), mersenne.mul(1, 1)); // (-1)^2 = 1
    }

    #[test]
    fn inv_is_correct() {
        let f = PrimeField::new(7);
        for a in 1..7u64 {
            let inv = f.inv(a).unwrap();
            assert_eq!(f.mul(a, inv), 1, "inv({a}) = {inv} is wrong");
        }
        assert_eq!(f.inv(0), None);
    }

    #[test]
    fn neg_is_correct() {
        let f = PrimeField::new(7);
        assert_eq!(f.neg(0), 0);
        assert_eq!(f.neg(3), 4); // -3 ≡ 4 (mod 7)
        assert_eq!(f.add(3, f.neg(3)), 0);
    }
}
