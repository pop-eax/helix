pub mod backend;
pub mod commit;
pub mod ir;
pub mod lowering;
pub mod net_backend;
pub mod ops;
pub mod runtime_field;
pub mod shamir;
pub mod types;

pub use backend::{BgwBackend, BgwConfig};
pub use net_backend::{
    count_multiplications, dealer_generate_triple_blobs, parse_triple_blob, BgwNetBackend,
};
pub use ops::{
    add_shares, generate_beaver_triple, multiply_shares, multiply_shares_with_triple, scale_shares,
    sub_shares, BeaverTriple, OpsError,
};
pub use runtime_field::PrimeField;
pub use shamir::{
    eval_polynomial, generate_shares_from_poly, reconstruct_secret, sample_polynomial, share_secret,
    ShamirError,
};
pub use types::{PartyShares, Share};

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn field() -> PrimeField {
        PrimeField::mersenne63()
    }

    #[test]
    fn share_and_reconstruct_round_trip() {
        let f = field();
        let mut rng = StdRng::seed_from_u64(42);
        let shares = share_secret(123, 3, 5, &f, &mut rng).unwrap();
        let recovered = reconstruct_secret(&shares.as_slice()[..3], &f).unwrap();
        assert_eq!(123u64, recovered);
    }

    #[test]
    fn add_shares_matches_clear_addition() {
        let f = field();
        let mut rng = StdRng::seed_from_u64(7);
        let a_shares = share_secret(10, 3, 5, &f, &mut rng).unwrap();
        let b_shares = share_secret(22, 3, 5, &f, &mut rng).unwrap();
        let c_shares = add_shares(a_shares.as_slice(), b_shares.as_slice(), &f).unwrap();
        let c = reconstruct_secret(&c_shares.as_slice()[..3], &f).unwrap();
        assert_eq!(c, 32u64);
    }

    #[test]
    fn multiply_shares_matches_clear_multiplication() {
        let f = field();
        let mut rng = StdRng::seed_from_u64(99);
        let a_shares = share_secret(6, 3, 5, &f, &mut rng).unwrap();
        let b_shares = share_secret(9, 3, 5, &f, &mut rng).unwrap();
        let p_shares = multiply_shares(a_shares.as_slice(), b_shares.as_slice(), 3, &f, &mut rng).unwrap();
        let p = reconstruct_secret(&p_shares.as_slice()[..3], &f).unwrap();
        assert_eq!(p, 54u64);
    }
}
