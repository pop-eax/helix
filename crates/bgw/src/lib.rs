pub mod backend;
pub mod field;
pub mod ir;
pub mod lowering;
pub mod net_backend;
pub mod ops;
pub mod shamir;
pub mod types;

pub use backend::{BgwBackend, BgwConfig};
pub use net_backend::{
    count_multiplications, dealer_generate_triple_blobs, parse_triple_blob, BgwNetBackend,
};
pub use field::{field_to_u64_checked, u64_to_field, FieldConversionError};
pub use ops::{
    add_shares, generate_beaver_triple, multiply_shares, multiply_shares_with_triple, scale_shares,
    sub_shares, BeaverTriple, OpsError,
};
pub use shamir::{
    eval_polynomial, generate_shares_from_poly, reconstruct_secret, sample_polynomial, share_secret,
    ShamirError,
};
pub use types::{PartyShares, Share};

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bls12_381::Fr;
    use ark_std::rand::rngs::StdRng;
    use ark_std::rand::SeedableRng;

    #[test]
    fn share_and_reconstruct_round_trip() {
        let mut rng = StdRng::seed_from_u64(42);
        let secret = Fr::from(123u64);
        let shares = share_secret(secret, 3, 5, &mut rng).unwrap();
        let recovered = reconstruct_secret(&shares.as_slice()[..3]).unwrap();
        assert_eq!(secret, recovered);
    }

    #[test]
    fn add_shares_matches_clear_addition() {
        let mut rng = StdRng::seed_from_u64(7);
        let a = Fr::from(10u64);
        let b = Fr::from(22u64);
        let a_shares = share_secret(a, 3, 5, &mut rng).unwrap();
        let b_shares = share_secret(b, 3, 5, &mut rng).unwrap();
        let c_shares = add_shares(a_shares.as_slice(), b_shares.as_slice()).unwrap();
        let c = reconstruct_secret(&c_shares.as_slice()[..3]).unwrap();
        assert_eq!(c, a + b);
    }

    #[test]
    fn multiply_shares_matches_clear_multiplication() {
        let mut rng = StdRng::seed_from_u64(99);
        let a = Fr::from(6u64);
        let b = Fr::from(9u64);
        let a_shares = share_secret(a, 3, 5, &mut rng).unwrap();
        let b_shares = share_secret(b, 3, 5, &mut rng).unwrap();
        let p_shares = multiply_shares(a_shares.as_slice(), b_shares.as_slice(), 3, &mut rng).unwrap();
        let p = reconstruct_secret(&p_shares.as_slice()[..3]).unwrap();
        assert_eq!(p, a * b);
    }
}
