use super::gate::Gate;
use serde::{Serialize, Deserialize};
use rayon::prelude::*;


#[derive(Clone, Serialize, Deserialize)]
pub struct Circuit {
    /// Flat label storage: labels[slot] = [label₀, label₁].
    /// Slot 0 is unused (reserved so that 0 is never a valid unallocated slot
    /// when used as an index — though the backend never uses slot 0 in practice).
    pub labels: Vec<[u128; 2]>,
    pub gates: Vec<Gate>,
    /// Slot indices of circuit-input bits (one per input bit).
    pub inputs: Vec<usize>,
    /// Slot indices of circuit-output bits.
    pub outputs: Vec<usize>,
    /// Total number of allocated slots (used by the evaluator to size its
    /// active-label Vec without needing the full label pairs).
    pub n_slots: usize,
}

impl Circuit {
    pub fn new() -> Self {
        Self {
            labels: Vec::new(),
            gates: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            n_slots: 0,
        }
    }

    /// Ensure slot `slot` exists in `labels` and has a valid label pair.
    /// Grows the Vec if needed; allocates a fresh random pair on first use.
    /// Returns the label pair.
    pub fn ensure_slot(&mut self, slot: usize) -> [u128; 2] {
        if slot >= self.labels.len() {
            self.labels.resize(slot + 1, [0u128; 2]);
        }
        if self.labels[slot] == [0u128; 2] {
            // Enforce color-bit convention: lsb(label₀) = 0, lsb(label₁) = 1.
            let l0 = super::random_label() & !1u128;
            let l1 = super::random_label() | 1u128;
            self.labels[slot] = [l0, l1];
            self.n_slots = self.n_slots.max(slot + 1);
        }
        self.labels[slot]
    }

    /// Register `slot` as a circuit input (allocates labels; idempotent).
    pub fn add_input_slot(&mut self, slot: usize) {
        self.ensure_slot(slot);
        if !self.inputs.contains(&slot) {
            self.inputs.push(slot);
        }
    }

    /// Register `slot` as a circuit output (allocates labels; idempotent).
    pub fn add_output_slot(&mut self, slot: usize) {
        self.ensure_slot(slot);
        if !self.outputs.contains(&slot) {
            self.outputs.push(slot);
        }
    }

    /// Return the active label for `slot` and `bit` (0 or 1).
    pub fn get_label(&self, slot: usize, bit: u8) -> Option<u128> {
        self.labels.get(slot).map(|l| l[bit as usize])
    }

    /// Add a garbled gate.  All referenced slots are lazily allocated on demand.
    pub fn add_gate(&mut self, logic_table: Vec<u8>, inputs: &[usize], output: usize) {
        for &s in inputs {
            self.ensure_slot(s);
        }
        self.ensure_slot(output);
        let mut gate = Gate::new(logic_table, inputs.to_vec(), output);
        gate.label_table(&self.labels);
        self.gates.push(gate);
    }

    pub fn garble(&mut self) -> Vec<Vec<(Vec<u8>, Vec<u8>)>> {
        self.gates
            .iter_mut()
            .map(|gate| gate.garble_table())
            .collect()
    }

    /// Evaluate the circuit.
    ///
    /// `active` is a Vec of length `n_slots`; each entry is `Some(label)` for
    /// slots whose active label is known (all inputs must be set before calling).
    /// Returns the same Vec with all output slots filled.
    pub fn evaluate(&self, mut active: Vec<Option<u128>>) -> Vec<Option<u128>> {
        for gate in &self.gates {
            let inputs: Vec<u128> = gate
                .input_slots()
                .iter()
                .map(|&s| active[s].expect("missing active label during evaluation"))
                .collect();
            if let Some(lbl) = gate.clone().evaluate(inputs) {
                active[gate.output_slot()] = Some(lbl);
            }
        }
        active
    }

    /// Compute the topological level of each gate.
    /// Returns a Vec parallel to `self.gates` (gate i → its level).
    pub fn compute_gate_levels(&self) -> Vec<u32> {
        let mut wire_level: Vec<u32> = vec![0u32; self.n_slots.max(1)];
        for &s in &self.inputs {
            if s < wire_level.len() {
                wire_level[s] = 0;
            }
        }

        let mut levels = Vec::with_capacity(self.gates.len());
        for gate in &self.gates {
            let lv = gate
                .input_slots()
                .iter()
                .map(|&s| if s < wire_level.len() { wire_level[s] } else { 0 })
                .max()
                .unwrap_or(0)
                + 1;
            let out = gate.output_slot();
            if out >= wire_level.len() {
                wire_level.resize(out + 1, 0);
            }
            wire_level[out] = lv;
            levels.push(lv);
        }
        levels
    }

    /// Level-parallel evaluation using rayon.
    pub fn evaluate_parallel(&self, mut active: Vec<Option<u128>>) -> Vec<Option<u128>> {
        if self.gates.is_empty() {
            return active;
        }

        let gate_levels = self.compute_gate_levels();
        let max_level = *gate_levels.iter().max().unwrap() as usize;

        let mut by_level: Vec<Vec<usize>> = vec![Vec::new(); max_level + 1];
        for (idx, &lv) in gate_levels.iter().enumerate() {
            by_level[lv as usize].push(idx);
        }

        for level_indices in &by_level {
            let new_labels: Vec<(usize, u128)> = level_indices
                .par_iter()
                .filter_map(|&idx| {
                    let gate = &self.gates[idx];
                    let inputs: Vec<u128> = gate
                        .input_slots()
                        .iter()
                        .map(|&s| active[s].expect("missing active label"))
                        .collect();
                    gate.clone()
                        .evaluate(inputs)
                        .map(|lbl| (gate.output_slot(), lbl))
                })
                .collect();
            for (slot, lbl) in new_labels {
                active[slot] = Some(lbl);
            }
        }

        active
    }
}
