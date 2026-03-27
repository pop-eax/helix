//! Oblivious Transfer — "Simplest OT" (Chou & Orlandi 2015) over Ristretto255.
//!
//! # Variants
//!
//! ## 1-of-2 OT  (`OTSender` / `OTReceiver`)
//!
//! Batched variant: each of `k` instances has two messages and one choice bit.
//! Used for Yao GC wire labels.
//!
//! ## 1-of-n OT  (`OT1OfNSender` / `OT1OfNReceiver`)
//!
//! Batched variant: each of `instances` runs picks **one** message out of a
//! table of `n`.  Running `k` instances over the same table implements k-of-n
//! OT — useful for MPC hash-map lookups where the receiver wants `k` entries
//! without revealing which ones.
//!
//! # Shared algebraic structure
//!
//! Both variants use the same Diffie-Hellman core, generalised by replacing
//! the binary choice encoding `σ·A` with an integer encoding `i·A`:
//!
//! - Sender picks `a`, sends `A = a·G`.
//! - Receiver with choice `i` picks `b`, sends `B = b·G + i·A`.
//! - Sender encrypts message `j` as `mⱼ XOR H(a·(B − j·A))`.
//!   - For `j = i`: `a·(B − i·A) = abG`      → matches receiver key `H(bA)` ✓
//!   - For `j ≠ i`: `a·(B − j·A) = abG + (i−j)·a²G` → unknown to receiver ✓
//!
//! 1-of-2 OT is the special case `n = 2`, `i ∈ {0, 1}`.

use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::CompressedRistretto,
    RistrettoPoint, Scalar,
};
use sha2::{Digest, Sha256};

// ---- internal helpers ----

/// Generate a random scalar using rand 0.9 (already a garbledc dep).
fn random_scalar() -> Scalar {
    use rand::Rng;
    let bytes: [u8; 32] = rand::rng().random();
    Scalar::from_bytes_mod_order(bytes)
}

/// Hash a Ristretto point down to a u128 mask (first 16 bytes of SHA-256).
fn hash_point(p: &RistrettoPoint) -> u128 {
    let digest = Sha256::digest(p.compress().as_bytes());
    u128::from_le_bytes(digest[..16].try_into().unwrap())
}

fn decompress(bytes: &[u8; 32]) -> RistrettoPoint {
    CompressedRistretto(*bytes)
        .decompress()
        .expect("invalid Ristretto point in OT message")
}

// ---- Sender (garbler) ----

/// Garbler-side OT state, created in round 1 and consumed in round 3.
pub struct OTSender {
    scalars: Vec<Scalar>,
    a_points: Vec<RistrettoPoint>,
}

impl OTSender {
    /// Round 1 — generate `(a, A=aG)` pairs for `n` OT instances.
    ///
    /// Returns `(state, a_bytes)`. Send `a_bytes` to the evaluator.
    pub fn setup(n: usize) -> (Self, Vec<[u8; 32]>) {
        let mut scalars = Vec::with_capacity(n);
        let mut a_points = Vec::with_capacity(n);
        let mut a_bytes = Vec::with_capacity(n);

        for _ in 0..n {
            let a = random_scalar();
            let a_pt = &a * RISTRETTO_BASEPOINT_POINT;
            a_bytes.push(*a_pt.compress().as_bytes());
            scalars.push(a);
            a_points.push(a_pt);
        }

        (Self { scalars, a_points }, a_bytes)
    }

    /// Round 3 — encrypt message pairs under the two possible receiver keys.
    ///
    /// `messages[i] = (m₀, m₁)`.  Returns `(enc_m₀, enc_m₁)` per instance.
    pub fn respond(
        self,
        b_bytes: &[[u8; 32]],
        messages: &[(u128, u128)],
    ) -> Vec<(u128, u128)> {
        assert_eq!(b_bytes.len(), self.scalars.len());
        assert_eq!(messages.len(), self.scalars.len());

        self.scalars
            .iter()
            .zip(self.a_points.iter())
            .zip(b_bytes.iter())
            .zip(messages.iter())
            .map(|(((a, a_pt), b_raw), (m0, m1))| {
                let b_pt = decompress(b_raw);
                // k0 = H(a·B);  k1 = H(a·(B−A))
                // When σ=0: B=bG → k0=H(abG), k1=H(a(bG−aG))  receiver key=H(bA)=H(abG)=k0 ✓
                // When σ=1: B=A+bG → k0=H(a(A+bG)), k1=H(abG)  receiver key=H(bA)=H(abG)=k1 ✓
                let k0 = hash_point(&(a * b_pt));
                let k1 = hash_point(&(a * (b_pt - a_pt)));
                (m0 ^ k0, m1 ^ k1)
            })
            .collect()
    }
}

// ---- Receiver (evaluator) ----

/// Evaluator-side OT state, created in round 2 and consumed after round 3.
pub struct OTReceiver {
    keys: Vec<u128>,
    choices: Vec<bool>,
}

impl OTReceiver {
    /// Round 2 — given the sender's A points and the choice bits, produce B points.
    ///
    /// Returns `(state, b_bytes)`. Send `b_bytes` to the sender.
    pub fn choose(a_bytes: &[[u8; 32]], choices: &[bool]) -> (Self, Vec<[u8; 32]>) {
        assert_eq!(a_bytes.len(), choices.len());

        let mut keys = Vec::with_capacity(choices.len());
        let mut b_bytes = Vec::with_capacity(choices.len());

        for (a_raw, &sigma) in a_bytes.iter().zip(choices.iter()) {
            let a_pt = decompress(a_raw);
            let b = random_scalar();
            let bg = &b * RISTRETTO_BASEPOINT_POINT;
            // σ=0 → B=bG  (no information about A leaks to sender via B)
            // σ=1 → B=A+bG (computationally indistinguishable from a random point)
            let b_pt = if sigma { a_pt + bg } else { bg };
            b_bytes.push(*b_pt.compress().as_bytes());
            // key = b·A = b·a·G = a·b·G  (same formula for both σ)
            keys.push(hash_point(&(b * a_pt)));
        }

        (Self { keys, choices: choices.to_vec() }, b_bytes)
    }

    /// Round 4 — decrypt the chosen message from each ciphertext pair.
    pub fn finish(self, ciphertexts: &[(u128, u128)]) -> Vec<u128> {
        assert_eq!(ciphertexts.len(), self.keys.len());

        ciphertexts
            .iter()
            .zip(self.keys.iter())
            .zip(self.choices.iter())
            .map(|((ct, key), &sigma)| {
                let enc = if sigma { ct.1 } else { ct.0 };
                enc ^ key
            })
            .collect()
    }
}

// ---- 1-of-n OT (k-of-n when running k instances) ----

/// Sender side of batched 1-of-n OT.
///
/// Run `instances` independent OT sessions, each letting the receiver obtain
/// one message out of a table of `n`.
///
/// **k-of-n OT** is `instances = k` over the same `n`-message table — the
/// receiver picks k distinct entries; the sender learns none of them.
pub struct OT1OfNSender {
    scalars: Vec<Scalar>,
    a_points: Vec<RistrettoPoint>,
    n: usize,
}

impl OT1OfNSender {
    /// Round 1 — generate `(a, A=aG)` for each of `instances` sessions.
    ///
    /// Returns `(state, a_bytes)`.  Send `a_bytes` to the receiver.
    pub fn setup(instances: usize, n: usize) -> (Self, Vec<[u8; 32]>) {
        let mut scalars = Vec::with_capacity(instances);
        let mut a_points = Vec::with_capacity(instances);
        let mut a_bytes = Vec::with_capacity(instances);

        for _ in 0..instances {
            let a = random_scalar();
            let a_pt = &a * RISTRETTO_BASEPOINT_POINT;
            a_bytes.push(*a_pt.compress().as_bytes());
            scalars.push(a);
            a_points.push(a_pt);
        }

        (Self { scalars, a_points, n }, a_bytes)
    }

    /// Round 3 — encrypt message tables.
    ///
    /// `b_bytes[i]` is the receiver's B-point for session i.
    /// `tables[i]` must contain exactly `n` messages.
    ///
    /// Returns `enc_tables[i][j] = messages[j] XOR H(a·(B − j·A))`.
    /// Only the entry at `j = choice[i]` decrypts correctly for the receiver.
    pub fn respond(self, b_bytes: &[[u8; 32]], tables: &[Vec<u128>]) -> Vec<Vec<u128>> {
        assert_eq!(b_bytes.len(), self.scalars.len());
        assert_eq!(tables.len(), self.scalars.len());

        self.scalars
            .iter()
            .zip(self.a_points.iter())
            .zip(b_bytes.iter())
            .zip(tables.iter())
            .map(|(((a, a_pt), b_raw), messages)| {
                assert_eq!(messages.len(), self.n, "table must have exactly n={} entries", self.n);
                let b_pt = decompress(b_raw);
                (0..self.n)
                    .map(|j| {
                        let j_sc = Scalar::from(j as u64);
                        // k_j = H(a · (B − j·A))
                        let k_j = hash_point(&(a * (b_pt - j_sc * *a_pt)));
                        messages[j] ^ k_j
                    })
                    .collect()
            })
            .collect()
    }
}

/// Receiver side of batched 1-of-n OT.
pub struct OT1OfNReceiver {
    keys: Vec<u128>,
    choices: Vec<usize>,
}

impl OT1OfNReceiver {
    /// Round 2 — compute B-points from A-points and choice indices.
    ///
    /// `choices[i] ∈ 0..n` is the index this session should retrieve.
    /// Returns `(state, b_bytes)`.  Send `b_bytes` to the sender.
    pub fn choose(a_bytes: &[[u8; 32]], choices: &[usize]) -> (Self, Vec<[u8; 32]>) {
        let mut keys = Vec::with_capacity(choices.len());
        let mut b_bytes = Vec::with_capacity(choices.len());

        for (a_raw, &choice) in a_bytes.iter().zip(choices.iter()) {
            let a_pt = decompress(a_raw);
            let b = random_scalar();
            let i_sc = Scalar::from(choice as u64);
            // B = b·G + i·A  (encodes choice i)
            let b_pt = &b * RISTRETTO_BASEPOINT_POINT + i_sc * a_pt;
            b_bytes.push(*b_pt.compress().as_bytes());
            // key = H(b·A) = H(b·a·G) = H(abG)
            keys.push(hash_point(&(b * a_pt)));
        }

        (Self { keys, choices: choices.to_vec() }, b_bytes)
    }

    /// Round 4 — decrypt the chosen entry from each encrypted table.
    ///
    /// Returns `messages[choice[i]]` for each session i.
    pub fn finish(self, enc_tables: &[Vec<u128>]) -> Vec<u128> {
        assert_eq!(enc_tables.len(), self.keys.len());

        enc_tables
            .iter()
            .zip(self.keys.iter())
            .zip(self.choices.iter())
            .map(|((table, key), &choice)| table[choice] ^ key)
            .collect()
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ot(m0: u128, m1: u128, sigma: bool) -> u128 {
        let (sender, a_bytes) = OTSender::setup(1);
        let (receiver, b_bytes) = OTReceiver::choose(&a_bytes, &[sigma]);
        let cts = sender.respond(&b_bytes, &[(m0, m1)]);
        receiver.finish(&cts)[0]
    }

    #[test]
    fn one_of_two_correct_message() {
        let (m0, m1) = (0xdeadbeef_u128, 0xcafebabe_u128);
        assert_eq!(run_ot(m0, m1, false), m0);
        assert_eq!(run_ot(m0, m1, true), m1);
    }

    #[test]
    fn one_of_two_batch() {
        let pairs: Vec<(u128, u128)> = (0..8).map(|i| (i * 2, i * 2 + 1)).collect();
        let choices = vec![false, true, false, true, false, true, false, true];

        let (sender, a_bytes) = OTSender::setup(8);
        let (receiver, b_bytes) = OTReceiver::choose(&a_bytes, &choices);
        let cts = sender.respond(&b_bytes, &pairs);
        let labels = receiver.finish(&cts);

        for (i, (&sigma, &got)) in choices.iter().zip(labels.iter()).enumerate() {
            let expected = if sigma { pairs[i].1 } else { pairs[i].0 };
            assert_eq!(got, expected, "OT instance {i} failed");
        }
    }

    #[test]
    fn one_of_n_each_index() {
        let n = 5;
        let table: Vec<u128> = (0..n as u128).map(|i| i * 0x1111_1111).collect();

        for choice in 0..n {
            let (sender, a_bytes) = OT1OfNSender::setup(1, n);
            let (receiver, b_bytes) = OT1OfNReceiver::choose(&a_bytes, &[choice]);
            let enc = sender.respond(&b_bytes, &[table.clone()]);
            let result = receiver.finish(&enc);
            assert_eq!(result[0], table[choice], "1-of-{n} failed for choice {choice}");
        }
    }

    #[test]
    fn k_of_n_hashmap_lookup() {
        // Simulate: sender has a 16-entry "hashmap" (index → value).
        // Receiver wants entries at indices 3, 7, 12 without revealing which.
        let n = 16;
        let table: Vec<u128> = (0..n as u128).map(|i| i * 0xabcd_ef01 + 0x1234).collect();
        let choices = vec![3usize, 7, 12];
        let k = choices.len();

        // All sessions reference the same table.
        let tables: Vec<Vec<u128>> = vec![table.clone(); k];

        let (sender, a_bytes) = OT1OfNSender::setup(k, n);
        let (receiver, b_bytes) = OT1OfNReceiver::choose(&a_bytes, &choices);
        let enc = sender.respond(&b_bytes, &tables);
        let results = receiver.finish(&enc);

        for (idx, (&choice, &got)) in choices.iter().zip(results.iter()).enumerate() {
            assert_eq!(got, table[choice], "k-of-n lookup {idx} (index {choice}) failed");
        }
    }

    #[test]
    fn receiver_cannot_decrypt_other_entries() {
        // The receiver's key H(abG) decrypts only enc_tables[choice];
        // all other entries XOR to garbage (almost certainly ≠ table[j]).
        let n = 4;
        let table: Vec<u128> = (0..n as u128).map(|i| i + 0xffff_0000).collect();
        let choice = 2usize;

        let (sender, a_bytes) = OT1OfNSender::setup(1, n);
        let (receiver, b_bytes) = OT1OfNReceiver::choose(&a_bytes, &[choice]);
        let enc = sender.respond(&b_bytes, &[table.clone()]);

        // Manually apply the receiver's key to every entry.
        let key = receiver.keys[0];
        let wrong_decryptions: Vec<u128> = enc[0].iter().enumerate()
            .filter(|&(j, _)| j != choice)
            .map(|(_, &ct)| ct ^ key)
            .collect();

        // Each wrong decryption should not equal the corresponding plaintext.
        for &dec in &wrong_decryptions {
            assert!(
                !table.contains(&dec),
                "receiver wrongly decrypted a non-chosen entry: {dec:#x}"
            );
        }
    }
}
