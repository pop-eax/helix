use crate::runtime_field::PrimeField;
use crate::shamir::{reconstruct_secret, share_secret, ShamirError};
use crate::types::{PartyShares, Share};
use rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpsError {
    LengthMismatch,
    Shamir(ShamirError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeaverTriple {
    pub a: PartyShares,
    pub b: PartyShares,
    pub c: PartyShares,
}

impl From<ShamirError> for OpsError {
    fn from(value: ShamirError) -> Self {
        OpsError::Shamir(value)
    }
}

fn validate_shapes(left: &[Share], right: &[Share]) -> Result<(), OpsError> {
    if left.len() != right.len() {
        return Err(OpsError::LengthMismatch);
    }
    Ok(())
}

pub fn add_shares(
    left: &[Share],
    right: &[Share],
    field: &PrimeField,
) -> Result<PartyShares, OpsError> {
    validate_shapes(left, right)?;
    Ok(PartyShares::new(
        left.iter()
            .zip(right)
            .map(|(a, b)| Share(field.add(a.0, b.0)))
            .collect(),
    ))
}

pub fn sub_shares(
    left: &[Share],
    right: &[Share],
    field: &PrimeField,
) -> Result<PartyShares, OpsError> {
    validate_shapes(left, right)?;
    Ok(PartyShares::new(
        left.iter()
            .zip(right)
            .map(|(a, b)| Share(field.sub(a.0, b.0)))
            .collect(),
    ))
}

pub fn scale_shares(shares: &[Share], k: u64, field: &PrimeField) -> PartyShares {
    PartyShares::new(shares.iter().map(|s| Share(field.mul(s.0, k))).collect())
}

pub fn generate_beaver_triple<R: Rng + ?Sized>(
    threshold: usize,
    parties: usize,
    field: &PrimeField,
    rng: &mut R,
) -> Result<BeaverTriple, OpsError> {
    let a_val = field.rand(rng);
    let b_val = field.rand(rng);
    let c_val = field.mul(a_val, b_val);
    Ok(BeaverTriple {
        a: share_secret(a_val, threshold, parties, field, rng)?,
        b: share_secret(b_val, threshold, parties, field, rng)?,
        c: share_secret(c_val, threshold, parties, field, rng)?,
    })
}

pub fn multiply_shares<R: Rng + ?Sized>(
    x: &[Share],
    y: &[Share],
    threshold: usize,
    field: &PrimeField,
    rng: &mut R,
) -> Result<PartyShares, OpsError> {
    validate_shapes(x, y)?;
    let triple = generate_beaver_triple(threshold, x.len(), field, rng)?;
    multiply_shares_with_triple(x, y, &triple, field)
}

/// [z] = [c] + δ·[b] + ε·[a] + δ·ε
pub fn multiply_shares_with_triple(
    x: &[Share],
    y: &[Share],
    triple: &BeaverTriple,
    field: &PrimeField,
) -> Result<PartyShares, OpsError> {
    validate_shapes(x, y)?;
    validate_shapes(x, triple.a.as_slice())?;
    validate_shapes(x, triple.b.as_slice())?;
    validate_shapes(x, triple.c.as_slice())?;

    let delta = reconstruct_secret(
        sub_shares(x, triple.a.as_slice(), field)?.as_slice(),
        field,
    )
    .map_err(OpsError::Shamir)?;

    let eta = reconstruct_secret(
        sub_shares(y, triple.b.as_slice(), field)?.as_slice(),
        field,
    )
    .map_err(OpsError::Shamir)?;

    let db = scale_shares(triple.b.as_slice(), delta, field);
    let ea = scale_shares(triple.a.as_slice(), eta, field);

    let mut out = add_shares(triple.c.as_slice(), db.as_slice(), field)?;
    out = add_shares(out.as_slice(), ea.as_slice(), field)?;

    let de = field.mul(delta, eta);
    Ok(PartyShares::new(
        out.as_slice()
            .iter()
            .map(|s| Share(field.add(s.0, de)))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_add_sub_mul_scalar() {
        let field = PrimeField::new(7);
        let a = Share(3);
        let b = Share(5);
        assert_eq!(add_shares(&[a], &[b], &field).unwrap().as_slice()[0].0, 1); // (3+5)%7=1
        assert_eq!(sub_shares(&[a], &[b], &field).unwrap().as_slice()[0].0, 5); // (3-5)%7=5
        let scaled = scale_shares(&[a], 3, &field);
        assert_eq!(scaled.as_slice()[0].0, 2); // (3*3)%7=2
    }
}
