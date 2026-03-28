//! Feldman VSS commitment utilities over BLS12-381 G1.
//!
//! When sharing a secret `v` with polynomial `p(x) = v + a₁x + ⋯ + aₜxᵗ`, the
//! sharer also publishes the commitment vector `[A₀, …, Aₜ]` where `Aⱼ = aⱼ·G`.
//!
//! Each receiving party verifies: `shareᵢ·G == ∑ Aⱼ·iʲ`.
//!
//! The secret commitment `A₀ = v·G` is additively homomorphic:
//! - `C(a+b) = C(a) + C(b)`
//! - `C(k·a) = k·C(a)`
//! - `C(a-b) = C(a) - C(b)`
//!
//! This lets us track commitments through linear gates and verify outputs on
//! linear circuits.  Multiplication gates break the linear chain (marked `None`).

use ark_bls12_381::{G1Affine, G1Projective, Fr};
use ark_ec::{CurveGroup, PrimeGroup};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::Zero;

/// Compressed BLS12-381 G1 point size.
pub const COMMIT_BYTES: usize = 48;

/// Standard BLS12-381 G1 generator.
pub fn generator() -> G1Projective {
    G1Projective::generator()
}

/// Compute the Feldman commitment vector: `Aⱼ = coeffs[j] · G`.
pub fn feldman_commitments(coeffs: &[Fr]) -> Vec<G1Projective> {
    let g = generator();
    coeffs.iter().map(|&c| g * c).collect()
}

/// Verify that `share` is a consistent evaluation of the committed polynomial at
/// `party_index` (1-based).
///
/// Checks: `share·G == ∑ commitments[j] · party_index^j`.
pub fn verify_share(share: Fr, party_index: usize, commitments: &[G1Projective]) -> bool {
    let g = generator();
    let lhs = g * share;
    let x = Fr::from(party_index as u64);
    let mut x_pow = Fr::from(1u64);
    let mut rhs = G1Projective::zero();
    for c in commitments {
        rhs += *c * x_pow;
        x_pow *= x;
    }
    lhs == rhs
}

/// `C(a+b) = C(a) + C(b)`.
pub fn add_commits(c1: G1Projective, c2: G1Projective) -> G1Projective {
    c1 + c2
}

/// `C(a-b) = C(a) - C(b)`.
pub fn sub_commits(c1: G1Projective, c2: G1Projective) -> G1Projective {
    c1 - c2
}

/// `C(k·a) = k · C(a)`.
pub fn scale_commit(c: G1Projective, k: Fr) -> G1Projective {
    c * k
}

/// Commitment to a public constant: `c·G` (no blinding needed for public values).
pub fn constant_commit(value: Fr) -> G1Projective {
    generator() * value
}

/// Serialise a G1Projective to 48 compressed bytes.
pub fn serialize_point(p: &G1Projective) -> [u8; COMMIT_BYTES] {
    let mut buf = [0u8; COMMIT_BYTES];
    p.into_affine()
        .serialize_compressed(&mut buf[..])
        .expect("serialize G1");
    buf
}

/// Deserialise 48 compressed bytes to G1Projective.
pub fn deserialize_point(bytes: &[u8]) -> Result<G1Projective, String> {
    G1Affine::deserialize_compressed(bytes)
        .map(Into::into)
        .map_err(|e| format!("G1 deserialize: {e}"))
}

/// Serialise a full commitment vector (concatenated compressed points).
pub fn serialize_commit_vec(cv: &[G1Projective]) -> Vec<u8> {
    cv.iter().flat_map(|p| serialize_point(p)).collect()
}

/// Deserialise a commitment vector.  `bytes.len()` must be a multiple of `COMMIT_BYTES`.
pub fn deserialize_commit_vec(bytes: &[u8]) -> Result<Vec<G1Projective>, String> {
    if bytes.len() % COMMIT_BYTES != 0 {
        return Err(format!(
            "commit vec length {} is not a multiple of {COMMIT_BYTES}",
            bytes.len()
        ));
    }
    bytes.chunks_exact(COMMIT_BYTES).map(deserialize_point).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::rand::rngs::StdRng;
    use ark_std::rand::SeedableRng;
    use crate::shamir::sample_and_share;

    #[test]
    fn verify_share_accepts_honest_share() {
        let mut rng = StdRng::seed_from_u64(1);
        let secret = Fr::from(42u64);
        let (shares, coeffs) = sample_and_share(secret, 2, 3, &mut rng).unwrap();
        let commits = feldman_commitments(&coeffs);
        for (i, share) in shares.as_slice().iter().enumerate() {
            assert!(verify_share(share.0, i + 1, &commits), "party {} failed", i + 1);
        }
    }

    #[test]
    fn verify_share_rejects_tampered_share() {
        let mut rng = StdRng::seed_from_u64(2);
        let secret = Fr::from(99u64);
        let (_, coeffs) = sample_and_share(secret, 2, 3, &mut rng).unwrap();
        let commits = feldman_commitments(&coeffs);
        let bad_share = Fr::from(0u64);
        assert!(!verify_share(bad_share, 1, &commits));
    }

    #[test]
    fn homomorphic_add_commit_matches() {
        let ca = constant_commit(Fr::from(10u64));
        let cb = constant_commit(Fr::from(20u64));
        let csum = add_commits(ca, cb);
        assert_eq!(csum, constant_commit(Fr::from(30u64)));
    }

    #[test]
    fn serialize_roundtrip() {
        let p = generator() * Fr::from(77u64);
        let bytes = serialize_point(&p);
        let p2 = deserialize_point(&bytes).unwrap();
        assert_eq!(p, p2);
    }
}
