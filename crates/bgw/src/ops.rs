use crate::shamir::{reconstruct_secret, share_secret, ShamirError};
use crate::types::{PartyShares, Share};
use ark_ff::PrimeField;
use ark_std::rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpsError {
    LengthMismatch,
    ShareXMismatch,
    Shamir(ShamirError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeaverTriple<F> {
    pub a: PartyShares<F>,
    pub b: PartyShares<F>,
    pub c: PartyShares<F>,
}

impl From<ShamirError> for OpsError {
    fn from(value: ShamirError) -> Self {
        OpsError::Shamir(value)
    }
}

fn validate_shapes<F: PrimeField>(left: &[Share<F>], right: &[Share<F>]) -> Result<(), OpsError> {
    if left.len() != right.len() {
        return Err(OpsError::LengthMismatch);
    }
    if left.iter().zip(right.iter()).any(|(l, r)| l.0 != r.0) {
        return Err(OpsError::ShareXMismatch);
    }
    Ok(())
}

pub fn add_shares<F: PrimeField>(
    left: &[Share<F>],
    right: &[Share<F>],
) -> Result<PartyShares<F>, OpsError> {
    validate_shapes(left, right)?;
    Ok(PartyShares::new(
        left.iter()
            .zip(right.iter())
            .map(|((x, ly), (_, ry))| (*x, *ly + *ry))
            .collect(),
    ))
}

pub fn sub_shares<F: PrimeField>(
    left: &[Share<F>],
    right: &[Share<F>],
) -> Result<PartyShares<F>, OpsError> {
    validate_shapes(left, right)?;
    Ok(PartyShares::new(
        left.iter()
            .zip(right.iter())
            .map(|((x, ly), (_, ry))| (*x, *ly - *ry))
            .collect(),
    ))
}

pub fn scale_shares<F: PrimeField>(shares: &[Share<F>], k: F) -> PartyShares<F> {
    PartyShares::new(shares.iter().map(|(x, y)| (*x, *y * k)).collect())
}

pub fn generate_beaver_triple<F: PrimeField, R: Rng + ?Sized>(
    threshold: usize,
    parties: usize,
    rng: &mut R,
) -> Result<BeaverTriple<F>, OpsError> {
    let a = F::rand(rng);
    let b = F::rand(rng);
    let c = a * b;
    Ok(BeaverTriple {
        a: share_secret(a, threshold, parties, rng)?,
        b: share_secret(b, threshold, parties, rng)?,
        c: share_secret(c, threshold, parties, rng)?,
    })
}

pub fn multiply_shares<F: PrimeField, R: Rng + ?Sized>(
    x: &[Share<F>],
    y: &[Share<F>],
    threshold: usize,
    rng: &mut R,
) -> Result<PartyShares<F>, OpsError> {
    validate_shapes(x, y)?;
    let triple = generate_beaver_triple(threshold, x.len(), rng)?;
    multiply_shares_with_triple(x, y, &triple)
}

// [z]= [c]+ δ⋅[b]+ϵ⋅[a]+δ⋅ϵ
pub fn multiply_shares_with_triple<F: PrimeField>(
    x: &[Share<F>],
    y: &[Share<F>],
    triple: &BeaverTriple<F>,
) -> Result<PartyShares<F>, OpsError> {
    validate_shapes(x, y)?;
    validate_shapes(x, triple.a.as_slice())?;
    validate_shapes(x, triple.b.as_slice())?;
    validate_shapes(x, triple.c.as_slice())?;

    let delta = reconstruct_secret(sub_shares(x, triple.a.as_slice())?.as_slice())?;
    let eta = reconstruct_secret(sub_shares(y, triple.b.as_slice())?.as_slice())?;

    let db = scale_shares(triple.b.as_slice(), delta);
    let ea = scale_shares(triple.a.as_slice(), eta);

    let mut out = add_shares(triple.c.as_slice(), db.as_slice())?;
    out = add_shares(out.as_slice(), ea.as_slice())?;

    Ok(PartyShares::new(
        out.as_slice()
            .iter()
            .map(|(x_i, z_i)| (*x_i, *z_i + delta * eta))
            .collect(),
    ))
}
