#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyShares<F>(Vec<Share<F>>);

pub type Share<F> = (F, F);

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
