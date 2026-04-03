use crate::runtime_field::PrimeField;
use crate::types::{PartyShares, Share};
use rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShamirError {
    InvalidThreshold,
    InvalidPartyCount,
    EmptyShares,
    DuplicateShareX,
}

/// Sample a random polynomial of degree `degree` with `secret` as the constant
/// term. Coefficients are field elements in `[0, field.modulus)`.
pub fn sample_polynomial<R: Rng + ?Sized>(
    secret: u64,
    degree: usize,
    field: &PrimeField,
    rng: &mut R,
) -> Vec<u64> {
    let mut coeffs = Vec::with_capacity(degree + 1);
    coeffs.push(secret);
    for _ in 0..degree {
        coeffs.push(field.rand(rng));
    }
    coeffs
}

/// Evaluate polynomial (given as coefficients) at `x` using Horner's method.
pub fn eval_polynomial(coeffs: &[u64], x: u64, field: &PrimeField) -> u64 {
    let mut acc = 0u64;
    for &c in coeffs.iter().rev() {
        acc = field.mul(acc, x);
        acc = field.add(acc, c);
    }
    acc
}

/// Evaluate `coeffs` at x = 1, 2, …, parties and return as `PartyShares`.
pub fn generate_shares_from_poly(
    coeffs: &[u64],
    parties: usize,
    field: &PrimeField,
) -> Result<PartyShares, ShamirError> {
    if parties == 0 {
        return Err(ShamirError::InvalidPartyCount);
    }
    let shares = (1..=parties)
        .map(|i| Share(eval_polynomial(coeffs, i as u64, field)))
        .collect();
    Ok(PartyShares::new(shares))
}

/// Like [`share_secret`] but also returns the polynomial coefficients (for
/// Feldman VSS or other purposes).
pub fn sample_and_share<R: Rng + ?Sized>(
    secret: u64,
    threshold: usize,
    parties: usize,
    field: &PrimeField,
    rng: &mut R,
) -> Result<(PartyShares, Vec<u64>), ShamirError> {
    if threshold == 0 {
        return Err(ShamirError::InvalidThreshold);
    }
    if parties < threshold {
        return Err(ShamirError::InvalidPartyCount);
    }
    let coeffs = sample_polynomial(secret, threshold - 1, field, rng);
    let shares = generate_shares_from_poly(&coeffs, parties, field)?;
    Ok((shares, coeffs))
}

/// Standard Shamir secret sharing: share `secret` among `parties` parties with
/// degree-`(threshold-1)` polynomial. Any `threshold` shares reconstruct.
pub fn share_secret<R: Rng + ?Sized>(
    secret: u64,
    threshold: usize,
    parties: usize,
    field: &PrimeField,
    rng: &mut R,
) -> Result<PartyShares, ShamirError> {
    if threshold == 0 {
        return Err(ShamirError::InvalidThreshold);
    }
    if parties < threshold {
        return Err(ShamirError::InvalidPartyCount);
    }
    let coeffs = sample_polynomial(secret, threshold - 1, field, rng);
    generate_shares_from_poly(&coeffs, parties, field)
}

/// Lagrange interpolation over all provided shares (party indices x = 1, …, n).
pub fn reconstruct_secret(shares: &[Share], field: &PrimeField) -> Result<u64, ShamirError> {
    if shares.is_empty() {
        return Err(ShamirError::EmptyShares);
    }
    let mut secret = 0u64;
    let n = shares.len();
    for i in 0..n {
        let x_i = (i + 1) as u64;
        let y_i = shares[i].0;
        // Lagrange basis polynomial numerator and denominator
        let mut num = 1u64;
        let mut den = 1u64;
        for j in 0..n {
            if i == j { continue; }
            let x_j = (j + 1) as u64;
            num = field.mul(num, field.neg(x_j));
            let diff = field.sub(x_i, x_j);
            den = field.mul(den, diff);
        }
        let inv_den = field.inv(den).ok_or(ShamirError::DuplicateShareX)?;
        let basis = field.mul(num, inv_den);
        let term = field.mul(y_i, basis);
        secret = field.add(secret, term);
    }
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn share_and_reconstruct_mod7() {
        let field = PrimeField::new(7);
        let mut rng = StdRng::seed_from_u64(1);
        let secret = 3u64;
        let shares = share_secret(secret, 3, 5, &field, &mut rng).unwrap();
        let recovered = reconstruct_secret(&shares.as_slice()[..3], &field).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn share_and_reconstruct_mersenne() {
        let field = PrimeField::mersenne63();
        let mut rng = StdRng::seed_from_u64(42);
        let secret = 123u64;
        let shares = share_secret(secret, 3, 5, &field, &mut rng).unwrap();
        let recovered = reconstruct_secret(&shares.as_slice()[..3], &field).unwrap();
        assert_eq!(recovered, secret);
    }
}
