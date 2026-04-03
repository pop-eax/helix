//! Networked BGW backend.
//!
//! Each party holds only its own Shamir share for every wire.  Communication
//! happens in two places:
//!
//! - **Multiplication**: one broadcast round using Beaver triples.
//!   Triples are generated via the trusted-dealer model: party 0 samples
//!   random triples and distributes one share per party before execution.
//!
//! - **Output reconstruction**: one broadcast round where every party sends its
//!   output-wire shares; all parties then run Lagrange interpolation locally.
//!
//! Add/Sub/constants are purely local — share arithmetic is linear.

use ir::lir::WireId;
use rand::rngs::OsRng;
use rand::RngCore;
use runtime::vm::{Backend, BackendError, Instruction, VMState, WireValue};
use runtime::Visibility;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::runtime_field::PrimeField;
use crate::shamir::{reconstruct_secret, sample_and_share};
use crate::types::Share;

// ---- Pending operation state ----

/// One Mul slot in a batched network round.
struct MulSlot {
    output_wire: WireId,
    a_i: u64,
    b_i: u64,
    c_i: u64,
    d_i: u64,
    e_i: u64,
}

/// One Select slot in a batched network round.
struct SelectSlot {
    output_wire: WireId,
    else_val_i:  u64,
    a_i: u64,
    b_i: u64,
    c_i: u64,
    d_i: u64,
    e_i: u64,
}

enum PendingOp {
    /// Batched Beaver round: all Mul and Select gates from one circuit level
    /// are processed in a single broadcast exchange.
    ///
    /// Wire format (per peer message):
    ///   4 bytes  n_muls    (u32 LE)
    ///   4 bytes  n_selects (u32 LE)
    ///   16 bytes per slot (d_i ‖ e_i, each 8 bytes LE u64)
    ///   Mul slots first, then Select slots.
    MulBatch {
        muls:    Vec<MulSlot>,
        selects: Vec<SelectSlot>,
    },
    /// Waiting for peers' output shares to reconstruct final values.
    Output { wires: Vec<WireId> },
}

// ---- Backend ----

pub struct BgwNetBackend {
    pub my_id: usize,
    pub n_parties: usize,
    pub threshold: usize,
    /// Runtime prime field for all share arithmetic.
    field: PrimeField,
    /// This party's share of every wire computed so far.
    my_shares: HashMap<WireId, u64>,
    /// Messages queued for the runner to send.
    outgoing: Vec<(usize, Vec<u8>)>,
    /// Current pending network operation (at most one at a time).
    pending: Option<PendingOp>,
    /// Pre-distributed Beaver triple shares (offline phase).
    /// Each entry is this party's share of one triple: (a_i, b_i, c_i).
    triple_queue: std::collections::VecDeque<(u64, u64, u64)>,
    /// Per-party OsRng for input sharing.
    input_rng: OsRng,
    output_cache: HashMap<WireId, u64>,
    /// 1-based party index for Shamir evaluation (= my_id + 1).
    party_index: usize,
    /// Enable SHA256 commitment verification on input shares.
    use_commits: bool,
}

impl BgwNetBackend {
    /// Create a new networked BGW backend with pre-distributed triple shares.
    ///
    /// `triple_shares` is this party's portion of the offline-phase triples —
    /// one `(a_i, b_i, c_i)` per multiplication gate.
    pub fn new(
        my_id: usize,
        n_parties: usize,
        threshold: usize,
        field: PrimeField,
        triple_shares: Vec<(u64, u64, u64)>,
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
            field,
            my_shares: HashMap::new(),
            outgoing: Vec::new(),
            pending: None,
            triple_queue: triple_shares.into(),
            input_rng: OsRng,
            output_cache: HashMap::new(),
            party_index: my_id + 1,
            use_commits: false,
        })
    }

    /// Enable SHA256 commitment to input shares.
    pub fn with_commits(mut self) -> Self {
        self.use_commits = true;
        self
    }

    fn pop_triple(&mut self) -> Result<(u64, u64, u64), BackendError> {
        self.triple_queue.pop_front().ok_or_else(|| {
            BackendError::BackendError(
                "Beaver triple queue exhausted — circuit has more multiplications \
                 than triples distributed in the offline phase"
                    .into(),
            )
        })
    }

    fn my_share(&self, wire: WireId) -> Result<u64, BackendError> {
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


impl Backend for BgwNetBackend {
    fn name(&self) -> &'static str {
        "BGW Arithmetic (networked)"
    }

    // ---- Input distribution ----

    /// Called by the party that OWNS this wire.
    /// Without commits: returns one 8-byte blob per party (the share).
    /// With commits: each blob is `[share (8)] ‖ [nonce (32)] ‖ [C_0 (32)] ‖ … ‖ [C_{n-1} (32)]`.
    fn share_input(
        &mut self,
        _wire: WireId,
        value: u64,
        n_parties: usize,
    ) -> Result<Vec<Vec<u8>>, BackendError> {
        let reduced = self.field.from_u64(value);
        let (all, _coeffs) = sample_and_share(
            reduced,
            self.threshold,
            n_parties,
            &self.field,
            &mut self.input_rng,
        )
        .map_err(|e| BackendError::BackendError(format!("share_input: {e:?}")))?;

        if !self.use_commits {
            return Ok(all
                .as_slice()
                .iter()
                .map(|s| PrimeField::to_bytes(s.0).to_vec())
                .collect());
        }

        // Build commit blobs: [share (8)] || [nonce (32)] || [C_0..C_{n-1} (32*n)]
        let shares = all.as_slice();
        let mut nonces: Vec<[u8; 32]> = Vec::with_capacity(n_parties);
        let mut commits: Vec<[u8; 32]> = Vec::with_capacity(n_parties);
        for s in shares {
            let mut nonce = [0u8; 32];
            self.input_rng.fill_bytes(&mut nonce);
            let share_bytes = PrimeField::to_bytes(s.0);
            let mut h = Sha256::new();
            h.update(share_bytes);
            h.update(nonce);
            let c: [u8; 32] = h.finalize().into();
            nonces.push(nonce);
            commits.push(c);
        }
        let commit_bytes: Vec<u8> = commits.iter().flat_map(|c| c.iter().copied()).collect();
        Ok(shares
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut blob = Vec::with_capacity(8 + 32 + 32 * n_parties);
                blob.extend(PrimeField::to_bytes(s.0));
                blob.extend(nonces[i]);
                blob.extend(&commit_bytes);
                blob
            })
            .collect())
    }

    /// Called with the blob pushed to us by the wire's owner.
    fn receive_input_share(
        &mut self,
        wire: WireId,
        _visibility: Visibility,
        share: Vec<u8>,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        if share.len() < 8 {
            return Err(BackendError::BackendError(format!(
                "share blob too short: {} bytes (expected ≥ 8)",
                share.len()
            )));
        }
        let share_val = PrimeField::from_bytes(&share[..8])?;

        // Verify SHA256 commitment if commit blob is present.
        if share.len() > 8 {
            if share.len() < 40 {
                return Err(BackendError::BackendError(
                    "commit blob too short for nonce".into(),
                ));
            }
            let nonce = &share[8..40];
            let commits_data = &share[40..];
            let n_commits = commits_data.len() / 32;
            let idx = self.my_id.min(n_commits.saturating_sub(1));
            if commits_data.len() < (idx + 1) * 32 {
                return Err(BackendError::BackendError(
                    "commit blob missing commit for this party".into(),
                ));
            }
            let expected_commit = &commits_data[idx * 32..(idx + 1) * 32];
            let mut h = Sha256::new();
            h.update(&share[..8]);
            h.update(nonce);
            let actual: [u8; 32] = h.finalize().into();
            if actual.as_slice() != expected_commit {
                return Err(BackendError::BackendError(format!(
                    "input share commitment mismatch for wire {:?}",
                    wire
                )));
            }
        }

        self.my_shares.insert(wire, share_val);
        state.set_wire(wire, WireValue::Secret, Visibility::Secret);
        Ok(())
    }

    // set_input is the single-party path (not used in networked mode but
    // provided for compatibility — shares the value locally).
    fn set_input(
        &mut self,
        wire: WireId,
        value: u64,
        visibility: Visibility,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        let reduced = self.field.from_u64(value);
        let (all, _coeffs) = sample_and_share(
            reduced,
            self.threshold,
            self.n_parties,
            &self.field,
            &mut self.input_rng,
        )
        .map_err(|e| BackendError::BackendError(format!("set_input: {e:?}")))?;
        self.my_shares.insert(wire, all.as_slice()[self.my_id].0);
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
                let z = self.field.add(self.my_share(*input1)?, self.my_share(*input2)?);
                self.my_shares.insert(*output, z);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
            }
            Instruction::Sub { vis, input1, input2, output, .. } => {
                let z = self.field.sub(self.my_share(*input1)?, self.my_share(*input2)?);
                self.my_shares.insert(*output, z);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
            }
            // Public constants: every party holds the constant itself as their share.
            // Reconstruction: Σ λ_i · c = c · Σ λ_i = c · 1 = c  ✓
            Instruction::Constant { value, output, visibility, .. } => {
                self.my_shares.insert(*output, self.field.from_u64(*value));
                state.set_wire(*output, WireValue::Secret, *visibility);
            }
            Instruction::AddConstant { vis, input, constant, output, .. } => {
                let z = self.field.add(self.my_share(*input)?, self.field.from_u64(*constant));
                self.my_shares.insert(*output, z);
                state.set_wire(*output, WireValue::Secret, *vis);
            }
            Instruction::SubConstant { vis, input, constant, output, .. } => {
                let z = self.field.sub(self.my_share(*input)?, self.field.from_u64(*constant));
                self.my_shares.insert(*output, z);
                state.set_wire(*output, WireValue::Secret, *vis);
            }
            Instruction::MulConstant { vis, input, constant, output, .. } => {
                let z = self.field.mul(self.my_share(*input)?, self.field.from_u64(*constant));
                self.my_shares.insert(*output, z);
                state.set_wire(*output, WireValue::Secret, *vis);
            }

            // Multiplication — single-gate path: wrap in a one-element MulBatch.
            Instruction::Mul { vis, input1, input2, output, .. } => {
                let x_i = self.my_share(*input1)?;
                let y_i = self.my_share(*input2)?;
                let (a_i, b_i, c_i) = self.pop_triple()?;
                let d_i = self.field.sub(x_i, a_i);
                let e_i = self.field.sub(y_i, b_i);

                let slot = MulSlot { output_wire: *output, a_i, b_i, c_i, d_i, e_i };
                let mut msg = Vec::with_capacity(8 + 16);
                msg.extend_from_slice(&1u32.to_le_bytes());
                msg.extend_from_slice(&0u32.to_le_bytes());
                msg.extend(PrimeField::to_bytes(d_i));
                msg.extend(PrimeField::to_bytes(e_i));
                self.broadcast(msg);

                self.pending = Some(PendingOp::MulBatch { muls: vec![slot], selects: vec![] });
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
            }

            // Select: output = else_val + condition * (then_val - else_val)
            Instruction::Select { output_vis, condition, then_val, else_val, output } => {
                let cond_i = self.my_share(*condition)?;
                let tv_i   = self.my_share(*then_val)?;
                let ev_i   = self.my_share(*else_val)?;
                let diff_i = self.field.sub(tv_i, ev_i);
                let (a_i, b_i, c_i) = self.pop_triple()?;
                let d_i = self.field.sub(cond_i, a_i);
                let e_i = self.field.sub(diff_i, b_i);

                let slot = SelectSlot {
                    output_wire: *output,
                    else_val_i: ev_i,
                    a_i, b_i, c_i, d_i, e_i,
                };
                let mut msg = Vec::with_capacity(8 + 16);
                msg.extend_from_slice(&0u32.to_le_bytes());
                msg.extend_from_slice(&1u32.to_le_bytes());
                msg.extend(PrimeField::to_bytes(d_i));
                msg.extend(PrimeField::to_bytes(e_i));
                self.broadcast(msg);

                self.pending = Some(PendingOp::MulBatch { muls: vec![], selects: vec![slot] });
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

    /// Level-batched execution: process all linear gates locally, then batch
    /// all Mul/Select gates into a single network round per circuit level.
    fn execute_batch(
        &mut self,
        instructions: &[Instruction],
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        let mut muls:    Vec<MulSlot>    = Vec::new();
        let mut selects: Vec<SelectSlot> = Vec::new();

        for instruction in instructions {
            match instruction {
                Instruction::Mul { vis, input1, input2, output, .. } => {
                    let x_i = self.my_share(*input1)?;
                    let y_i = self.my_share(*input2)?;
                    let (a_i, b_i, c_i) = self.pop_triple()?;
                    let d_i = self.field.sub(x_i, a_i);
                    let e_i = self.field.sub(y_i, b_i);
                    muls.push(MulSlot { output_wire: *output, a_i, b_i, c_i, d_i, e_i });
                    state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                }
                Instruction::Select { output_vis, condition, then_val, else_val, output } => {
                    let cond_i = self.my_share(*condition)?;
                    let tv_i   = self.my_share(*then_val)?;
                    let ev_i   = self.my_share(*else_val)?;
                    let diff_i = self.field.sub(tv_i, ev_i);
                    let (a_i, b_i, c_i) = self.pop_triple()?;
                    let d_i = self.field.sub(cond_i, a_i);
                    let e_i = self.field.sub(diff_i, b_i);
                    selects.push(SelectSlot {
                        output_wire: *output,
                        else_val_i: ev_i,
                        a_i, b_i, c_i, d_i, e_i,
                    });
                    state.set_wire(*output, WireValue::Secret, *output_vis);
                }
                other => {
                    // All linear gates are local.
                    self.execute_instruction(other, state)?;
                }
            }
        }

        if !muls.is_empty() || !selects.is_empty() {
            let n_muls    = muls.len() as u32;
            let n_selects = selects.len() as u32;
            let total     = (n_muls + n_selects) as usize;
            let mut msg   = Vec::with_capacity(8 + total * 16);
            msg.extend_from_slice(&n_muls.to_le_bytes());
            msg.extend_from_slice(&n_selects.to_le_bytes());
            for slot in &muls {
                msg.extend(PrimeField::to_bytes(slot.d_i));
                msg.extend(PrimeField::to_bytes(slot.e_i));
            }
            for slot in &selects {
                msg.extend(PrimeField::to_bytes(slot.d_i));
                msg.extend(PrimeField::to_bytes(slot.e_i));
            }
            self.broadcast(msg);
            self.pending = Some(PendingOp::MulBatch { muls, selects });
        }

        Ok(())
    }

    fn take_outgoing(&mut self) -> Vec<(usize, Vec<u8>)> {
        std::mem::take(&mut self.outgoing)
    }

    fn receive_replies(&mut self, messages: Vec<(usize, Vec<u8>)>) -> Result<(), BackendError> {
        match self.pending.take() {
            // ---- Batched Mul+Select: reconstruct all δ_k, ε_k in one pass ----
            Some(PendingOp::MulBatch { muls, selects }) => {
                let n         = self.n_parties;
                let n_muls    = muls.len();
                let n_selects = selects.len();
                let total     = n_muls + n_selects;

                // d_shares[k][party] and e_shares[k][party] for slot k.
                let mut d_shares: Vec<Vec<Share>> = vec![vec![Share(0u64); n]; total];
                let mut e_shares: Vec<Vec<Share>> = vec![vec![Share(0u64); n]; total];

                // Fill in our own contributions.
                for (k, slot) in muls.iter().enumerate() {
                    d_shares[k][self.my_id] = Share(slot.d_i);
                    e_shares[k][self.my_id] = Share(slot.e_i);
                }
                for (k, slot) in selects.iter().enumerate() {
                    d_shares[n_muls + k][self.my_id] = Share(slot.d_i);
                    e_shares[n_muls + k][self.my_id] = Share(slot.e_i);
                }

                // Parse peer messages.
                for (from, msg) in &messages {
                    if msg.len() < 8 {
                        return Err(BackendError::BackendError(
                            "MulBatch reply too short (< 8 bytes header)".into(),
                        ));
                    }
                    let peer_n_muls    = u32::from_le_bytes(msg[0..4].try_into().unwrap()) as usize;
                    let peer_n_selects = u32::from_le_bytes(msg[4..8].try_into().unwrap()) as usize;
                    let peer_total     = peer_n_muls + peer_n_selects;
                    let expected_len   = 8 + peer_total * 16;
                    if msg.len() < expected_len {
                        return Err(BackendError::BackendError(format!(
                            "MulBatch reply from party {from} too short: {} < {expected_len}",
                            msg.len()
                        )));
                    }
                    for k in 0..peer_total {
                        let off = 8 + k * 16;
                        d_shares[k][*from] = Share(PrimeField::from_bytes(&msg[off..off + 8])?);
                        e_shares[k][*from] = Share(PrimeField::from_bytes(&msg[off + 8..off + 16])?);
                    }
                }

                // Reconstruct each Mul slot: z_i = c_i + δ·b_i + ε·a_i + δ·ε
                for (k, slot) in muls.iter().enumerate() {
                    let delta = reconstruct_secret(&d_shares[k], &self.field)
                        .map_err(|e| BackendError::BackendError(format!("reconstruct δ[{k}]: {e:?}")))?;
                    let eta = reconstruct_secret(&e_shares[k], &self.field)
                        .map_err(|e| BackendError::BackendError(format!("reconstruct ε[{k}]: {e:?}")))?;
                    let z_i = self.field.add(
                        slot.c_i,
                        self.field.add(
                            self.field.mul(delta, slot.b_i),
                            self.field.add(
                                self.field.mul(eta, slot.a_i),
                                self.field.mul(delta, eta),
                            ),
                        ),
                    );
                    self.my_shares.insert(slot.output_wire, z_i);
                }

                // Reconstruct each Select slot.
                for (k, slot) in selects.iter().enumerate() {
                    let delta = reconstruct_secret(&d_shares[n_muls + k], &self.field)
                        .map_err(|e| BackendError::BackendError(format!("reconstruct δ_sel[{k}]: {e:?}")))?;
                    let eta = reconstruct_secret(&e_shares[n_muls + k], &self.field)
                        .map_err(|e| BackendError::BackendError(format!("reconstruct ε_sel[{k}]: {e:?}")))?;
                    let cond_diff_i = self.field.add(
                        slot.c_i,
                        self.field.add(
                            self.field.mul(delta, slot.b_i),
                            self.field.add(
                                self.field.mul(eta, slot.a_i),
                                self.field.mul(delta, eta),
                            ),
                        ),
                    );
                    let result_i = self.field.add(slot.else_val_i, cond_diff_i);
                    self.my_shares.insert(slot.output_wire, result_i);
                }
            }

            // ---- Output reconstruction: collect shares, reconstruct ----
            Some(PendingOp::Output { wires }) => {
                let n = self.n_parties;

                for (wire_idx, &wire) in wires.iter().enumerate() {
                    let mut all = vec![Share(0u64); n];
                    all[self.my_id] = Share(self.my_share(wire)?);

                    for (from, msg) in &messages {
                        let offset = wire_idx * 8;
                        if msg.len() < offset + 8 {
                            return Err(BackendError::BackendError(
                                "output reply too short".into(),
                            ));
                        }
                        all[*from] = Share(PrimeField::from_bytes(&msg[offset..offset + 8])?);
                    }

                    let secret = reconstruct_secret(&all, &self.field)
                        .map_err(|e| BackendError::BackendError(format!("reconstruct output: {e:?}")))?;
                    self.output_cache.insert(wire, secret);
                }
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
        let mut msg = Vec::with_capacity(wires.len() * 8);
        for &wire in wires {
            msg.extend(PrimeField::to_bytes(self.my_share(wire)?));
        }
        self.broadcast(msg);
        self.pending = Some(PendingOp::Output { wires: wires.to_vec() });
        Ok(())
    }

    fn get_output(&mut self, wire: WireId, _state: &VMState) -> Result<u64, BackendError> {
        self.output_cache
            .get(&wire)
            .copied()
            .ok_or(BackendError::WireNotSet(wire))
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

/// Dealer (party 0): generate `n_triples` Beaver triples and return one
/// serialised blob per party.  Blob `i` contains `([a]_i ‖ [b]_i ‖ [c]_i)`
/// for each triple — 24 bytes per triple.
pub fn dealer_generate_triple_blobs(
    n_triples: usize,
    n_parties: usize,
    threshold: usize,
    field: &PrimeField,
) -> Vec<Vec<u8>> {
    use crate::ops::generate_beaver_triple;
    let mut blobs: Vec<Vec<u8>> = vec![Vec::with_capacity(n_triples * 24); n_parties];
    let mut rng = OsRng;

    for _ in 0..n_triples {
        let triple = generate_beaver_triple(threshold, n_parties, field, &mut rng)
            .expect("triple generation");
        for i in 0..n_parties {
            blobs[i].extend(PrimeField::to_bytes(triple.a.as_slice()[i].0));
            blobs[i].extend(PrimeField::to_bytes(triple.b.as_slice()[i].0));
            blobs[i].extend(PrimeField::to_bytes(triple.c.as_slice()[i].0));
        }
    }
    blobs
}

/// Parse a blob received from the dealer into a vector of triple shares.
/// Each 24-byte chunk is one `(a_i, b_i, c_i)`.
pub fn parse_triple_blob(blob: &[u8]) -> Result<Vec<(u64, u64, u64)>, BackendError> {
    if blob.len() % 24 != 0 {
        return Err(BackendError::BackendError(format!(
            "triple blob length {} is not a multiple of 24",
            blob.len()
        )));
    }
    blob.chunks_exact(24)
        .map(|chunk| {
            let a = PrimeField::from_bytes(&chunk[..8])?;
            let b = PrimeField::from_bytes(&chunk[8..16])?;
            let c = PrimeField::from_bytes(&chunk[16..24])?;
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

    fn test_field() -> PrimeField {
        PrimeField::mersenne63()
    }

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

    fn mul_program() -> ir::lir::Program {
        let mut b = CircuitBuilder::new();
        let w0 = b.add_input(Visibility::Secret, Some("a".into()));
        let w1 = b.add_input(Visibility::Secret, Some("b".into()));
        let out = b.add_gate(GateType::Mul, vec![w0, w1]);
        b.add_output(out);
        b.build(metadata("mul"))
    }

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
        let field = test_field();
        let shares = parse_triple_blob(&blobs[id]).unwrap();
        BgwNetBackend::new(id, parties, threshold, field, shares).unwrap()
    }

    #[tokio::test]
    async fn three_party_bgw_multiplication() {
        let program = mul_program();
        let (parties, threshold) = (3, 2);
        let field = test_field();

        let n_muls = count_multiplications(&program);
        let blobs = dealer_generate_triple_blobs(n_muls, parties, threshold, &field);

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

    #[tokio::test]
    async fn three_party_bgw_add_then_mul() {
        let program = add_mul_program();
        let (parties, threshold) = (3, 2);
        let field = test_field();

        let n_muls = count_multiplications(&program);
        let blobs = dealer_generate_triple_blobs(n_muls, parties, threshold, &field);

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

    #[tokio::test]
    async fn three_party_bgw_add_only() {
        let mut builder = CircuitBuilder::new();
        let w0 = builder.add_input(Visibility::Secret, Some("a".into()));
        let w1 = builder.add_input(Visibility::Secret, Some("b".into()));
        let out = builder.add_gate(GateType::Add, vec![w0, w1]);
        builder.add_output(out);
        let program = builder.build(metadata("add_only"));

        let (parties, threshold) = (3, 2);
        let field = test_field();
        let blobs = dealer_generate_triple_blobs(0, parties, threshold, &field);

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

        assert_eq!(t0.await.unwrap()[0].1, 33);
        assert_eq!(t1.await.unwrap()[0].1, 33);
        assert_eq!(t2.await.unwrap()[0].1, 33);
    }
}
