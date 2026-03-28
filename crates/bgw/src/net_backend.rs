//! Networked BGW backend.
//!
//! Each party holds only its own Shamir share for every wire.  Communication
//! happens in two places:
//!
//! - **Multiplication**: one broadcast round using Beaver triples.
//!   Triples are generated via the trusted-dealer model: party 0 samples a
//!   random seed with `OsRng` and broadcasts it to all parties before protocol
//!   execution begins (see [`dealer_seed`] + [`BgwNetBackend::new`]).  All
//!   parties initialise the same `StdRng` from that seed and call
//!   `generate_beaver_triple` in lockstep, each taking slice `[my_id]`.
//!
//!   Input sharing uses a separate per-party `OsRng` so that unequal input
//!   counts across parties cannot desynchronise the shared triple-generation
//!   sequence.
//!
//! - **Output reconstruction**: one broadcast round where every party sends its
//!   output-wire shares; all parties then run Lagrange interpolation locally.
//!
//! Add/Sub/constants are purely local — share arithmetic is linear.

use ark_bls12_381::{Fr, G1Projective};
use ark_ff::PrimeField;
use ir::lir::WireId;
use runtime::vm::{Backend, BackendError, Instruction, VMState, WireValue};
use runtime::Visibility;
use std::collections::HashMap;

use crate::commit;
use crate::field::{field_to_u64_checked, u64_to_field};
use crate::shamir::{reconstruct_secret, sample_and_share};
use crate::types::Share;

// ---- Fr serialisation (32 bytes, 4 × little-endian u64 limbs) ----

fn fr_to_bytes(f: Fr) -> Vec<u8> {
    let bigint = f.into_bigint();
    bigint.as_ref().iter().flat_map(|&l| l.to_le_bytes()).collect()
}

fn fr_from_bytes(bytes: &[u8]) -> Result<Fr, BackendError> {
    if bytes.len() != 32 {
        return Err(BackendError::BackendError(format!(
            "expected 32 bytes for Fr, got {}",
            bytes.len()
        )));
    }
    let mut limbs = [0u64; 4];
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        limbs[i] = u64::from_le_bytes(chunk.try_into().unwrap());
    }
    Fr::from_bigint(ark_ff::BigInt(limbs))
        .ok_or_else(|| BackendError::BackendError("Fr deserialization: value out of range".into()))
}

// ---- Pending operation state ----

enum PendingOp {
    /// Waiting for peers' (d_j, e_j) shares to finish a multiplication.
    Mul {
        output_wire: WireId,
        a_i: Share<Fr>, // my Beaver triple share of a
        b_i: Share<Fr>, // my Beaver triple share of b
        c_i: Share<Fr>, // my Beaver triple share of c = a*b
        d_i: Fr,        // my share of δ = x − a
        e_i: Fr,        // my share of ε = y − b
    },
    /// Select: same Beaver round as Mul for (cond * diff); then add else_val_i.
    /// output = else_val + cond * (then_val - else_val)
    Select {
        output_wire: WireId,
        else_val_i: Share<Fr>, // my share of else_val, added after the mul
        a_i: Share<Fr>,
        b_i: Share<Fr>,
        c_i: Share<Fr>,
        d_i: Fr,
        e_i: Fr,
    },
    /// Waiting for peers' output shares to reconstruct final values.
    Output { wires: Vec<WireId> },
}

// ---- Backend ----

pub struct BgwNetBackend {
    pub my_id: usize,
    pub n_parties: usize,
    pub threshold: usize,
    /// This party's share of every wire computed so far.
    my_shares: HashMap<WireId, Share<Fr>>,
    /// Messages queued for the runner to send.
    outgoing: Vec<(usize, Vec<u8>)>,
    /// Current pending network operation (at most one at a time).
    pending: Option<PendingOp>,
    /// Pre-distributed Beaver triple shares (offline phase).
    /// Each entry is this party's share of one triple: ([a]_i, [b]_i, [c]_i).
    /// Popped in order as Mul / Select gates are evaluated.
    triple_queue: std::collections::VecDeque<(Share<Fr>, Share<Fr>, Share<Fr>)>,
    /// Per-party OsRng for input sharing — independent across parties.
    input_rng: ark_std::rand::rngs::OsRng,
    output_cache: HashMap<WireId, u64>,
    /// Feldman VSS commitment shadow: `Some(C)` = wire has a live commitment,
    /// `None` = commitment was broken by a Mul/Select gate.
    wire_commit: HashMap<WireId, Option<G1Projective>>,
    /// 1-based party index for Shamir evaluation (= my_id + 1).
    party_index: usize,
}

impl BgwNetBackend {
    /// Create a new networked BGW backend with pre-distributed triple shares.
    ///
    /// `triple_shares` is this party's portion of the offline-phase triples —
    /// one `(a_i, b_i, c_i)` per multiplication gate.  Obtain it by calling
    /// [`dealer_distribute_triples`] on party 0 before starting the runner.
    pub fn new(
        my_id: usize,
        n_parties: usize,
        threshold: usize,
        triple_shares: Vec<(Share<Fr>, Share<Fr>, Share<Fr>)>,
    ) -> Result<Self, BackendError> {
        if n_parties == 0 {
            return Err(BackendError::BackendError("n_parties must be > 0".into()));
        }
        if threshold == 0 || threshold > n_parties {
            return Err(BackendError::BackendError(
                "threshold must satisfy 0 < threshold ≤ n_parties".into(),
            ));
        }
        if my_id >= n_parties {
            return Err(BackendError::BackendError(format!(
                "my_id {my_id} out of range for {n_parties} parties"
            )));
        }
        Ok(Self {
            my_id,
            n_parties,
            threshold,
            my_shares: HashMap::new(),
            outgoing: Vec::new(),
            pending: None,
            triple_queue: triple_shares.into(),
            input_rng: ark_std::rand::rngs::OsRng,
            output_cache: HashMap::new(),
            wire_commit: HashMap::new(),
            party_index: my_id + 1,
        })
    }

    fn pop_triple(&mut self) -> Result<(Share<Fr>, Share<Fr>, Share<Fr>), BackendError> {
        self.triple_queue.pop_front().ok_or_else(|| {
            BackendError::BackendError(
                "Beaver triple queue exhausted — circuit has more multiplications \
                 than triples distributed in the offline phase"
                    .into(),
            )
        })
    }

    fn my_share(&self, wire: WireId) -> Result<Share<Fr>, BackendError> {
        self.my_shares
            .get(&wire)
            .copied()
            .ok_or(BackendError::WireNotSet(wire))
    }

    /// Queue one message per peer (broadcast pattern).
    fn broadcast(&mut self, msg: Vec<u8>) {
        for j in 0..self.n_parties {
            if j != self.my_id {
                self.outgoing.push((j, msg.clone()));
            }
        }
    }
}

// ---- Commitment propagation helpers ----

/// `C(a+b) = C(a) + C(b)` — returns `None` if either input has no commitment.
fn opt_add_commits(a: Option<G1Projective>, b: Option<G1Projective>) -> Option<G1Projective> {
    match (a, b) {
        (Some(ca), Some(cb)) => Some(commit::add_commits(ca, cb)),
        _ => None,
    }
}

/// `C(a-b) = C(a) - C(b)` — returns `None` if either input has no commitment.
fn opt_sub_commits(a: Option<G1Projective>, b: Option<G1Projective>) -> Option<G1Projective> {
    match (a, b) {
        (Some(ca), Some(cb)) => Some(commit::sub_commits(ca, cb)),
        _ => None,
    }
}

impl Backend for BgwNetBackend {
    fn name(&self) -> &'static str {
        "BGW Arithmetic (networked)"
    }

    // ---- Input distribution ----

    /// Called by the party that OWNS this wire.
    /// Returns one serialised blob per party: `[32 bytes: Fr share] ++ [(t+1)*48 bytes: Feldman commit vec]`.
    fn share_input(
        &mut self,
        _wire: WireId,
        value: u64,
        n_parties: usize,
    ) -> Result<Vec<Vec<u8>>, BackendError> {
        let (all, coeffs) = sample_and_share(
            u64_to_field::<Fr>(value),
            self.threshold,
            n_parties,
            &mut self.input_rng,
        )
        .map_err(|e| BackendError::BackendError(format!("share_input: {e:?}")))?;
        let commit_vec = commit::feldman_commitments(&coeffs);
        let commit_bytes = commit::serialize_commit_vec(&commit_vec);
        Ok(all
            .into_inner()
            .into_iter()
            .map(|s| {
                let mut blob = fr_to_bytes(s.0);
                blob.extend_from_slice(&commit_bytes);
                blob
            })
            .collect())
    }

    /// Called with the blob pushed to us by the wire's owner.
    /// Blob format: `[32 bytes: Fr share] ++ [(t+1)*48 bytes: Feldman commit vec]`.
    fn receive_input_share(
        &mut self,
        wire: WireId,
        _visibility: Visibility,
        share: Vec<u8>,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        if share.len() < 32 {
            return Err(BackendError::BackendError(format!(
                "share blob too short: {} bytes",
                share.len()
            )));
        }
        let s = fr_from_bytes(&share[..32])?;
        let commit_opt = if share.len() > 32 {
            let commit_vec = commit::deserialize_commit_vec(&share[32..])
                .map_err(|e| BackendError::BackendError(e))?;
            if !commit::verify_share(s, self.party_index, &commit_vec) {
                return Err(BackendError::BackendError(
                    "Feldman VSS share verification failed — input may have been tampered with"
                        .into(),
                ));
            }
            Some(commit_vec[0])
        } else {
            None
        };
        self.my_shares.insert(wire, Share(s));
        self.wire_commit.insert(wire, commit_opt);
        state.set_wire(wire, WireValue::Secret, Visibility::Secret);
        Ok(())
    }

    // set_input is the single-party path — not used in networked mode but
    // provided for compatibility (shares the value locally).
    fn set_input(
        &mut self,
        wire: WireId,
        value: u64,
        visibility: Visibility,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        let (all, coeffs) = sample_and_share(
            u64_to_field::<Fr>(value),
            self.threshold,
            self.n_parties,
            &mut self.input_rng,
        )
        .map_err(|e| BackendError::BackendError(format!("set_input: {e:?}")))?;
        let commit_vec = commit::feldman_commitments(&coeffs);
        self.wire_commit.insert(wire, Some(commit_vec[0]));
        self.my_shares.insert(wire, all.as_slice()[self.my_id]);
        state.set_wire(wire, WireValue::Secret, visibility);
        Ok(())
    }

    // ---- Gate execution ----

    fn execute_instruction(
        &mut self,
        instruction: &Instruction,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        match instruction {
            // Linear gates — local arithmetic, no communication.
            Instruction::Add { vis, input1, input2, output, .. } => {
                let z = Share(self.my_share(*input1)?.0 + self.my_share(*input2)?.0);
                self.my_shares.insert(*output, z);
                let c = opt_add_commits(
                    self.wire_commit.get(input1).and_then(|x| x.as_ref()).cloned(),
                    self.wire_commit.get(input2).and_then(|x| x.as_ref()).cloned(),
                );
                self.wire_commit.insert(*output, c);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
            }
            Instruction::Sub { vis, input1, input2, output, .. } => {
                let z = Share(self.my_share(*input1)?.0 - self.my_share(*input2)?.0);
                self.my_shares.insert(*output, z);
                let c = opt_sub_commits(
                    self.wire_commit.get(input1).and_then(|x| x.as_ref()).cloned(),
                    self.wire_commit.get(input2).and_then(|x| x.as_ref()).cloned(),
                );
                self.wire_commit.insert(*output, c);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
            }
            // Public constants: every party holds the constant itself.
            // Reconstruction: Σ λ_i · c = c · Σ λ_i = c · 1 = c  ✓
            Instruction::Constant { value, output, visibility, .. } => {
                self.my_shares.insert(*output, Share(u64_to_field::<Fr>(*value)));
                self.wire_commit.insert(
                    *output,
                    Some(commit::constant_commit(u64_to_field::<Fr>(*value))),
                );
                state.set_wire(*output, WireValue::Secret, *visibility);
            }
            // Constant-operand gates: every party applies the same linear op.
            Instruction::AddConstant { vis, input, constant, output, .. } => {
                let c_fr = u64_to_field::<Fr>(*constant);
                let z = Share(self.my_share(*input)?.0 + c_fr);
                self.my_shares.insert(*output, z);
                let wc = self
                    .wire_commit
                    .get(input)
                    .and_then(|x| x.as_ref())
                    .cloned()
                    .map(|c| commit::add_commits(c, commit::constant_commit(c_fr)));
                self.wire_commit.insert(*output, wc);
                state.set_wire(*output, WireValue::Secret, *vis);
            }
            Instruction::SubConstant { vis, input, constant, output, .. } => {
                let c_fr = u64_to_field::<Fr>(*constant);
                let z = Share(self.my_share(*input)?.0 - c_fr);
                self.my_shares.insert(*output, z);
                let wc = self
                    .wire_commit
                    .get(input)
                    .and_then(|x| x.as_ref())
                    .cloned()
                    .map(|c| commit::sub_commits(c, commit::constant_commit(c_fr)));
                self.wire_commit.insert(*output, wc);
                state.set_wire(*output, WireValue::Secret, *vis);
            }
            Instruction::MulConstant { vis, input, constant, output, .. } => {
                let c_fr = u64_to_field::<Fr>(*constant);
                let z = Share(self.my_share(*input)?.0 * c_fr);
                self.my_shares.insert(*output, z);
                let wc = self
                    .wire_commit
                    .get(input)
                    .and_then(|x| x.as_ref())
                    .cloned()
                    .map(|c| commit::scale_commit(c, c_fr));
                self.wire_commit.insert(*output, wc);
                state.set_wire(*output, WireValue::Secret, *vis);
            }

            // Multiplication — needs one broadcast round (Beaver triples).
            // Breaks the commitment chain: output commitment is marked None.
            Instruction::Mul { vis, input1, input2, output, .. } => {
                let x_i = self.my_share(*input1)?;
                let y_i = self.my_share(*input2)?;

                let (a_i, b_i, c_i) = self.pop_triple()?;

                let d_i = x_i.0 - a_i.0; // my share of δ = x − a
                let e_i = y_i.0 - b_i.0; // my share of ε = y − b

                // Broadcast (d_i, e_i) so every party can reconstruct δ, ε.
                let mut msg = fr_to_bytes(d_i);
                msg.extend(fr_to_bytes(e_i));
                self.broadcast(msg);

                self.pending = Some(PendingOp::Mul {
                    output_wire: *output,
                    a_i,
                    b_i,
                    c_i,
                    d_i,
                    e_i,
                });
                self.wire_commit.insert(*output, None); // Mul breaks linearity
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
            }

            // Select: output = else_val + condition * (then_val - else_val)
            // The subtraction is local; the multiply needs one Beaver round.
            // Breaks the commitment chain: output commitment is marked None.
            Instruction::Select { output_vis, condition, then_val, else_val, output } => {
                let cond_i = self.my_share(*condition)?;
                let tv_i = self.my_share(*then_val)?;
                let ev_i = self.my_share(*else_val)?;

                // diff_i = then_val_i - else_val_i  (local)
                let diff_i = Share(tv_i.0 - ev_i.0);

                let (a_i, b_i, c_i) = self.pop_triple()?;

                let d_i = cond_i.0 - a_i.0;
                let e_i = diff_i.0 - b_i.0;

                let mut msg = fr_to_bytes(d_i);
                msg.extend(fr_to_bytes(e_i));
                self.broadcast(msg);

                self.pending = Some(PendingOp::Select {
                    output_wire: *output,
                    else_val_i: ev_i,
                    a_i,
                    b_i,
                    c_i,
                    d_i,
                    e_i,
                });
                self.wire_commit.insert(*output, None); // Select breaks linearity
                state.set_wire(*output, WireValue::Secret, *output_vis);
            }

            Instruction::LessThan { .. } | Instruction::Equal { .. } => {
                return Err(BackendError::BackendError(
                    "comparison gates require the Yao backend (garbled circuits); \
                     not supported in arithmetic BGW MPC"
                        .into(),
                ));
            }

            other => {
                return Err(BackendError::BackendError(format!(
                    "BgwNetBackend: instruction {other:?} not supported"
                )))
            }
        }
        Ok(())
    }

    fn take_outgoing(&mut self) -> Vec<(usize, Vec<u8>)> {
        std::mem::take(&mut self.outgoing)
    }

    fn receive_replies(&mut self, messages: Vec<(usize, Vec<u8>)>) -> Result<(), BackendError> {
        match self.pending.take() {
            // ---- Multiply: reconstruct δ and ε, compute z_i ----
            Some(PendingOp::Mul { output_wire, a_i, b_i, c_i, d_i, e_i }) => {
                let n = self.n_parties;
                let mut d_shares = vec![Share(Fr::from(0u64)); n];
                let mut e_shares = vec![Share(Fr::from(0u64)); n];

                d_shares[self.my_id] = Share(d_i);
                e_shares[self.my_id] = Share(e_i);

                for (from, msg) in &messages {
                    if msg.len() < 64 {
                        return Err(BackendError::BackendError(
                            "Mul reply too short (expected 64 bytes)".into(),
                        ));
                    }
                    d_shares[*from] = Share(fr_from_bytes(&msg[..32])?);
                    e_shares[*from] = Share(fr_from_bytes(&msg[32..64])?);
                }

                let delta = reconstruct_secret(&d_shares)
                    .map_err(|e| BackendError::BackendError(format!("reconstruct δ: {e:?}")))?;
                let eta = reconstruct_secret(&e_shares)
                    .map_err(|e| BackendError::BackendError(format!("reconstruct ε: {e:?}")))?;

                // [z]_i = [c]_i + δ·[b]_i + ε·[a]_i + δ·ε
                let z_i = c_i.0 + delta * b_i.0 + eta * a_i.0 + delta * eta;
                self.my_shares.insert(output_wire, Share(z_i));
            }

            // ---- Select: same Beaver reconstruction as Mul, then add else_val ----
            Some(PendingOp::Select { output_wire, else_val_i, a_i, b_i, c_i, d_i, e_i }) => {
                let n = self.n_parties;
                let mut d_shares = vec![Share(Fr::from(0u64)); n];
                let mut e_shares = vec![Share(Fr::from(0u64)); n];

                d_shares[self.my_id] = Share(d_i);
                e_shares[self.my_id] = Share(e_i);

                for (from, msg) in &messages {
                    if msg.len() < 64 {
                        return Err(BackendError::BackendError(
                            "Select reply too short (expected 64 bytes)".into(),
                        ));
                    }
                    d_shares[*from] = Share(fr_from_bytes(&msg[..32])?);
                    e_shares[*from] = Share(fr_from_bytes(&msg[32..64])?);
                }

                let delta = reconstruct_secret(&d_shares)
                    .map_err(|e| BackendError::BackendError(format!("select reconstruct δ: {e:?}")))?;
                let eta = reconstruct_secret(&e_shares)
                    .map_err(|e| BackendError::BackendError(format!("select reconstruct ε: {e:?}")))?;

                // cond_diff_i = c_i + δ·b_i + ε·a_i + δ·ε
                let cond_diff_i = c_i.0 + delta * b_i.0 + eta * a_i.0 + delta * eta;
                // result_i = else_val_i + cond_diff_i
                let result_i = else_val_i.0 + cond_diff_i;
                self.my_shares.insert(output_wire, Share(result_i));
            }

            // ---- Output reconstruction: collect shares, reconstruct ----
            Some(PendingOp::Output { wires }) => {
                let n = self.n_parties;
                let n_wires = wires.len();

                for (wire_idx, &wire) in wires.iter().enumerate() {
                    let mut all = vec![Share(Fr::from(0u64)); n];
                    all[self.my_id] = self.my_share(wire)?;

                    for (from, msg) in &messages {
                        let offset = wire_idx * 32;
                        if msg.len() < offset + 32 {
                            return Err(BackendError::BackendError(
                                "output reply too short".into(),
                            ));
                        }
                        all[*from] = Share(fr_from_bytes(&msg[offset..offset + 32])?);
                    }

                    let secret = reconstruct_secret(&all)
                        .map_err(|e| BackendError::BackendError(format!("reconstruct output: {e:?}")))?;
                    let value = field_to_u64_checked(secret).map_err(|e| {
                        BackendError::BackendError(format!("output u64 conversion: {e:?}"))
                    })?;
                    self.output_cache.insert(wire, value);
                }

                // Suppress unused variable warning
                let _ = n_wires;
            }

            None => {} // add/sub/constant — no replies expected
        }
        Ok(())
    }

    fn prepare_output_reconstruction(
        &mut self,
        wires: &[WireId],
        _state: &VMState,
    ) -> Result<(), BackendError> {
        // Pack all my output shares into one message, broadcast to every peer.
        let mut msg = Vec::with_capacity(wires.len() * 32);
        for &wire in wires {
            msg.extend(fr_to_bytes(self.my_share(wire)?.0));
        }
        self.broadcast(msg);
        self.pending = Some(PendingOp::Output { wires: wires.to_vec() });
        Ok(())
    }

    fn get_output(&mut self, wire: WireId, _state: &VMState) -> Result<u64, BackendError> {
        let v = self
            .output_cache
            .get(&wire)
            .copied()
            .ok_or(BackendError::WireNotSet(wire))?;

        // Homomorphic output check: if the commitment chain survived (no Mul gate
        // between input and output), verify v·G == stored commitment.
        if let Some(&Some(expected)) = self.wire_commit.get(&wire) {
            let actual = commit::constant_commit(u64_to_field::<Fr>(v));
            if actual != expected {
                return Err(BackendError::BackendError(format!(
                    "output commitment mismatch on wire {:?}: \
                     homomorphic verification failed (wire may have been tampered with)",
                    wire
                )));
            }
        }

        Ok(v)
    }
}

// ---- Trusted-dealer offline phase ----

/// Count the number of Beaver triples the circuit needs (one per Mul / Select gate).
pub fn count_multiplications(program: &ir::lir::Program) -> usize {
    use ir::lir::GateType;
    program
        .circuit
        .gates
        .iter()
        .filter(|g| matches!(g.gate_type, GateType::Mul | GateType::Select))
        .count()
}

/// Dealer (party 0): generate `n_triples` Beaver triples with `OsRng` and
/// return one serialised blob per party.  Blob `i` contains
/// `([a]_i ‖ [b]_i ‖ [c]_i)` for each triple — 96 bytes per triple.
/// Send `blobs[i]` to party `i` (keep `blobs[my_id]` locally).
pub fn dealer_generate_triple_blobs(
    n_triples: usize,
    n_parties: usize,
    threshold: usize,
) -> Vec<Vec<u8>> {
    use crate::ops::generate_beaver_triple;
    use ark_std::rand::rngs::OsRng;

    let mut blobs: Vec<Vec<u8>> = vec![Vec::with_capacity(n_triples * 96); n_parties];
    let mut rng = OsRng;

    for _ in 0..n_triples {
        let triple = generate_beaver_triple::<Fr, _>(threshold, n_parties, &mut rng)
            .expect("triple generation");
        for i in 0..n_parties {
            blobs[i].extend(fr_to_bytes(triple.a.as_slice()[i].0));
            blobs[i].extend(fr_to_bytes(triple.b.as_slice()[i].0));
            blobs[i].extend(fr_to_bytes(triple.c.as_slice()[i].0));
        }
    }
    blobs
}

/// Parse a blob received from the dealer into a vector of triple shares.
/// Each 96-byte chunk is one `([a]_i, [b]_i, [c]_i)`.
pub fn parse_triple_blob(
    blob: &[u8],
) -> Result<Vec<(Share<Fr>, Share<Fr>, Share<Fr>)>, BackendError> {
    if blob.len() % 96 != 0 {
        return Err(BackendError::BackendError(format!(
            "triple blob length {} is not a multiple of 96",
            blob.len()
        )));
    }
    blob.chunks_exact(96)
        .map(|chunk| {
            let a = Share(fr_from_bytes(&chunk[..32])?);
            let b = Share(fr_from_bytes(&chunk[32..64])?);
            let c = Share(fr_from_bytes(&chunk[64..96])?);
            Ok((a, b, c))
        })
        .collect()
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use ir::lir::{CircuitBuilder, GateType, Metadata, Statistics, Visibility, WireId};
    use net::stub_networks;
    use runtime::{InputAssignment, Runner};

    fn metadata(name: &str) -> Metadata {
        Metadata {
            version: "test".into(),
            source_file: "test".into(),
            function_name: name.into(),
            field_modulus: None,
            statistics: Statistics {
                total_gates: 0,
                gate_counts: Default::default(),
                circuit_depth: 0,
                num_inputs: 0,
                num_outputs: 0,
                num_wires: 0,
            },
        }
    }

    /// Circuit: output = a * b
    fn mul_program() -> ir::lir::Program {
        let mut b = CircuitBuilder::new();
        let w0 = b.add_input(Visibility::Secret, Some("a".into()));
        let w1 = b.add_input(Visibility::Secret, Some("b".into()));
        let out = b.add_gate(GateType::Mul, vec![w0, w1]);
        b.add_output(out);
        b.build(metadata("mul"))
    }

    /// Circuit: output = (a + b) * c
    fn add_mul_program() -> ir::lir::Program {
        let mut b = CircuitBuilder::new();
        let w0 = b.add_input(Visibility::Secret, Some("a".into()));
        let w1 = b.add_input(Visibility::Secret, Some("b".into()));
        let w2 = b.add_input(Visibility::Secret, Some("c".into()));
        let sum = b.add_gate(GateType::Add, vec![w0, w1]);
        let out = b.add_gate(GateType::Mul, vec![sum, w2]);
        b.add_output(out);
        b.build(metadata("add_mul"))
    }

    fn make_backend(
        id: usize,
        parties: usize,
        threshold: usize,
        blobs: &[Vec<u8>],
    ) -> BgwNetBackend {
        let shares = parse_triple_blob(&blobs[id]).unwrap();
        BgwNetBackend::new(id, parties, threshold, shares).unwrap()
    }

    /// 3-party BGW: a * b = 6 * 7 = 42.
    /// Party 0 owns `a`, party 1 owns `b`, party 2 is compute-only.
    #[tokio::test]
    async fn three_party_bgw_multiplication() {
        let program = mul_program();
        let (parties, threshold) = (3, 2);

        let n_muls = count_multiplications(&program);
        let blobs = dealer_generate_triple_blobs(n_muls, parties, threshold);

        let mut stubs = stub_networks(parties);
        let (net0, net1, net2) = (stubs.remove(0), stubs.remove(0), stubs.remove(0));
        let (p0, p1, p2) = (program.clone(), program.clone(), program.clone());

        let mk = |v0: Option<u64>, v1: Option<u64>| {
            vec![
                InputAssignment { wire: WireId(0), owner: 0, value: v0 },
                InputAssignment { wire: WireId(1), owner: 1, value: v1 },
            ]
        };

        let b0 = make_backend(0, parties, threshold, &blobs);
        let b1 = make_backend(1, parties, threshold, &blobs);
        let b2 = make_backend(2, parties, threshold, &blobs);

        let t0 = tokio::spawn(async move {
            Runner::new(net0, b0, p0, &mk(Some(6), None)).unwrap().run().await.unwrap()
        });
        let t1 = tokio::spawn(async move {
            Runner::new(net1, b1, p1, &mk(None, Some(7))).unwrap().run().await.unwrap()
        });
        let t2 = tokio::spawn(async move {
            Runner::new(net2, b2, p2, &mk(None, None)).unwrap().run().await.unwrap()
        });

        assert_eq!(t0.await.unwrap()[0].1, 42);
        assert_eq!(t1.await.unwrap()[0].1, 42);
        assert_eq!(t2.await.unwrap()[0].1, 42);
    }

    /// 3-party BGW: (a + b) * c = (3 + 4) * 5 = 35.
    /// Party 0 owns `a` and `b`, party 1 owns `c`, party 2 is compute-only.
    /// Add is local (no triple); Mul uses one Beaver triple.
    #[tokio::test]
    async fn three_party_bgw_add_then_mul() {
        let program = add_mul_program();
        let (parties, threshold) = (3, 2);

        let n_muls = count_multiplications(&program);
        let blobs = dealer_generate_triple_blobs(n_muls, parties, threshold);

        let mut stubs = stub_networks(parties);
        let (net0, net1, net2) = (stubs.remove(0), stubs.remove(0), stubs.remove(0));
        let (p0, p1, p2) = (program.clone(), program.clone(), program.clone());

        let mk = |v0: Option<u64>, v1: Option<u64>, v2: Option<u64>| {
            vec![
                InputAssignment { wire: WireId(0), owner: 0, value: v0 },
                InputAssignment { wire: WireId(1), owner: 0, value: v1 },
                InputAssignment { wire: WireId(2), owner: 1, value: v2 },
            ]
        };

        let b0 = make_backend(0, parties, threshold, &blobs);
        let b1 = make_backend(1, parties, threshold, &blobs);
        let b2 = make_backend(2, parties, threshold, &blobs);

        let t0 = tokio::spawn(async move {
            Runner::new(net0, b0, p0, &mk(Some(3), Some(4), None)).unwrap().run().await.unwrap()
        });
        let t1 = tokio::spawn(async move {
            Runner::new(net1, b1, p1, &mk(None, None, Some(5))).unwrap().run().await.unwrap()
        });
        let t2 = tokio::spawn(async move {
            Runner::new(net2, b2, p2, &mk(None, None, None)).unwrap().run().await.unwrap()
        });

        assert_eq!(t0.await.unwrap()[0].1, 35);
        assert_eq!(t1.await.unwrap()[0].1, 35);
        assert_eq!(t2.await.unwrap()[0].1, 35);
    }

    /// 3-party BGW: pure-add circuit `a + b` — no Mul, so the commitment chain
    /// stays intact and the homomorphic output check fires silently.
    #[tokio::test]
    async fn three_party_bgw_add_homomorphic_check() {
        let mut builder = CircuitBuilder::new();
        let w0 = builder.add_input(Visibility::Secret, Some("a".into()));
        let w1 = builder.add_input(Visibility::Secret, Some("b".into()));
        let out = builder.add_gate(GateType::Add, vec![w0, w1]);
        builder.add_output(out);
        let program = builder.build(metadata("add_only"));

        let (parties, threshold) = (3, 2);
        let blobs = dealer_generate_triple_blobs(0, parties, threshold); // no triples needed

        let mut stubs = stub_networks(parties);
        let (net0, net1, net2) = (stubs.remove(0), stubs.remove(0), stubs.remove(0));
        let (p0, p1, p2) = (program.clone(), program.clone(), program.clone());

        let mk = |v0: Option<u64>, v1: Option<u64>| {
            vec![
                InputAssignment { wire: WireId(0), owner: 0, value: v0 },
                InputAssignment { wire: WireId(1), owner: 1, value: v1 },
            ]
        };

        let b0 = make_backend(0, parties, threshold, &blobs);
        let b1 = make_backend(1, parties, threshold, &blobs);
        let b2 = make_backend(2, parties, threshold, &blobs);

        let t0 = tokio::spawn(async move {
            Runner::new(net0, b0, p0, &mk(Some(11), None)).unwrap().run().await.unwrap()
        });
        let t1 = tokio::spawn(async move {
            Runner::new(net1, b1, p1, &mk(None, Some(22))).unwrap().run().await.unwrap()
        });
        let t2 = tokio::spawn(async move {
            Runner::new(net2, b2, p2, &mk(None, None)).unwrap().run().await.unwrap()
        });

        // 11 + 22 = 33 — commitment check passed (no error) means homomorphic
        // verification succeeded end-to-end.
        assert_eq!(t0.await.unwrap()[0].1, 33);
        assert_eq!(t1.await.unwrap()[0].1, 33);
        assert_eq!(t2.await.unwrap()[0].1, 33);
    }

    /// Tamper test: corrupt a share blob so the Fr value doesn't satisfy the
    /// Feldman commitment — `receive_input_share` must return an error.
    #[test]
    fn feldman_tamper_detected() {
        // Build a valid blob for party 0 (party_index = 1).
        let (parties, threshold) = (3, 2);
        let blobs = dealer_generate_triple_blobs(0, parties, threshold);
        let mut backend = make_backend(0, parties, threshold, &blobs);

        // Construct a valid blob via share_input so we know the format.
        let valid_blobs = backend.share_input(WireId(0), 42, parties).unwrap();
        let mut tampered = valid_blobs[0].clone();
        // Flip one byte of the Fr share portion to corrupt it.
        tampered[0] ^= 0xFF;

        let mut state = runtime::vm::VMState::new(1, u64::MAX);
        let result = backend.receive_input_share(
            WireId(0),
            Visibility::Secret,
            tampered,
            &mut state,
        );
        assert!(
            result.is_err(),
            "tampered share should be rejected by Feldman verification"
        );
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("Feldman"), "expected Feldman error, got: {err_msg}");
    }
}
