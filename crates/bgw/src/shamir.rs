use crate::types::{PartyShares, Share};
use ark_ff::{Field, PrimeField};
use ark_std::rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShamirError {
    InvalidThreshold,
    InvalidPartyCount,
    EmptyShares,
    DuplicateShareX,
}

pub fn sample_polynomial<F: PrimeField, R: Rng + ?Sized>(
    secret: F,
    degree: usize,
    rng: &mut R,
) -> Vec<F> {
    let mut coeffs = Vec::with_capacity(degree + 1);
    coeffs.push(secret);
    for _ in 0..degree {
        coeffs.push(F::rand(rng));
    }
    coeffs
}

// Horner: (((a_{d} * x + a_{d-1}) * x + ...) * x + a0)
pub fn eval_polynomial<F: Field>(coeffs: &[F], x: F) -> F {
    let mut acc = F::zero();
    for &c in coeffs.iter().rev() {
        acc *= x;
        acc += c;
    }
    acc
}

pub fn generate_shares_from_poly<F: PrimeField>(
    coeffs: &[F],
    parties: usize,
) -> Result<PartyShares<F>, ShamirError> {
    if parties == 0 {
        return Err(ShamirError::InvalidPartyCount);
    }
    let shares = (1..=parties)
        .map(|i| {
            let x = F::from(i as u64);
            let y = eval_polynomial(coeffs, x);
            (x, y)
        })
        .collect();
    Ok(PartyShares::new(shares))
}

pub fn share_secret<F: PrimeField, R: Rng + ?Sized>(
    secret: F,
    threshold: usize,
    parties: usize,
    rng: &mut R,
) -> Result<PartyShares<F>, ShamirError> {
    if threshold == 0 {
        return Err(ShamirError::InvalidThreshold);
    }
    if parties < threshold {
        return Err(ShamirError::InvalidPartyCount);
    }
    let coeffs = sample_polynomial(secret, threshold - 1, rng);
    generate_shares_from_poly(&coeffs, parties)
}

pub fn reconstruct_secret<F: PrimeField>(shares: &[Share<F>]) -> Result<F, ShamirError> {
    if shares.is_empty() {
        return Err(ShamirError::EmptyShares);
    }
    let mut secret = F::zero();
    for (i, (x_i, y_i)) in shares.iter().enumerate() {
        let mut num = F::one();
        let mut den = F::one();
        for (j, (x_j, _)) in shares.iter().enumerate() {
            if i == j {
                continue;
            }
            if x_i == x_j {
                return Err(ShamirError::DuplicateShareX);
            }
            num *= -*x_j;
            den *= *x_i - *x_j;
        }
        secret += *y_i * (num * den.inverse().ok_or(ShamirError::DuplicateShareX)?);
    }
    Ok(secret)
}
