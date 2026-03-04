use ark_ff::PrimeField;
use std::ops::{Add, Mul, Neg, Sub};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyShares<F>(Vec<Share<F>>);

/// A single party's share: just the y (share value). The x (party index)
/// is implicit from the position in `PartyShares`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Share<F>(pub F);

impl<F: PrimeField> Add for Share<F> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self { Share(self.0 + rhs.0) }
}

impl<F: PrimeField> Sub for Share<F> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self { Share(self.0 - rhs.0) }
}

impl<F: PrimeField> Mul<F> for Share<F> {
    type Output = Self;
    fn mul(self, scalar: F) -> Self { Share(self.0 * scalar) }
}

impl<F: PrimeField> Neg for Share<F> {
    type Output = Self;
    fn neg(self) -> Self { Share(-self.0) }
}

impl<F> PartyShares<F> {
    pub fn new(shares: Vec<Share<F>>) -> Self {
        Self(shares)
    }

    pub fn as_slice(&self) -> &[Share<F>] {
        &self.0
    }

    pub fn into_inner(self) -> Vec<Share<F>> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
