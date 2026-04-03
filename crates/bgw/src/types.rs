/// A single party's Shamir share: the y-value of the sharing polynomial
/// evaluated at that party's index (x = party_index + 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Share(pub u64);

/// All parties' shares for a single wire value.
/// Party `i` holds `shares[i]`, evaluated at `x = i + 1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyShares(Vec<Share>);

impl PartyShares {
    pub fn new(shares: Vec<Share>) -> Self {
        Self(shares)
    }

    pub fn as_slice(&self) -> &[Share] {
        &self.0
    }

    pub fn into_inner(self) -> Vec<Share> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
