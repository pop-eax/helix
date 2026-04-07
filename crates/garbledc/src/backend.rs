use crate::circuit::Circuit;
use crate::gate::{and_logic, not_logic, or_logic, xor_logic};
use ir::lir::WireId;
use runtime::vm::{Backend, BackendError, Instruction, VMState, WireValue};
use runtime::Visibility;
use std::collections::HashMap;

pub struct YaoBackend {
    circuit: Circuit,
    bit_width: usize,
    /// Maps WireId → base slot in the flat label Vec.
    /// Wire `w` bit `b` lives at slot `wire_map[w] + b`.
    wire_map: HashMap<WireId, usize>,
    /// Monotonically increasing slot counter (covers both wire slots and
    /// single-slot intermediates like carry/cmp wires).
    next_slot: usize,
    /// Active input labels indexed by slot.  `None` means the slot's active
    /// label has not been selected yet (evaluator inputs arrive via OT).
    input_labels: Vec<Option<u128>>,
    garbled: bool,
    evaluation_cache: HashMap<WireId, u64>,
    /// Counter for allocating collision-free temporary wire IDs.
    next_temp_wire: usize,
}

impl YaoBackend {
    pub fn new(bit_width: usize) -> Self {
        Self {
            circuit: Circuit::new(),
            bit_width,
            wire_map: HashMap::new(),
            next_slot: 1, // slot 0 reserved; first real slot is 1
            input_labels: Vec::new(),
            garbled: false,
            evaluation_cache: HashMap::new(),
            next_temp_wire: 1_000_000,
        }
    }

    // ── Slot allocation ───────────────────────────────────────────────────────

    /// Allocate `bit_width` consecutive slots for `wire` and return the base.
    /// Idempotent — returns existing base if `wire` was already allocated.
    fn alloc_wire(&mut self, wire: WireId) -> usize {
        if let Some(&base) = self.wire_map.get(&wire) {
            return base;
        }
        let base = self.next_slot;
        self.next_slot += self.bit_width;
        self.wire_map.insert(wire, base);
        // Eagerly create label pairs for all bits of this wire.
        for b in 0..self.bit_width {
            self.circuit.ensure_slot(base + b);
        }
        base
    }

    /// Return slot index for `wire` bit `bit`.
    fn wire_slot(&self, wire: WireId, bit: usize) -> usize {
        self.wire_map[&wire] + bit
    }

    /// Allocate one slot for an intermediate (carry, cmp, etc.) wire.
    /// Labels are allocated lazily when the slot is first used in `add_gate`.
    fn alloc_single_slot(&mut self) -> usize {
        let s = self.next_slot;
        self.next_slot += 1;
        s
    }

    /// Grow `input_labels` to cover all allocated slots.
    fn sync_input_labels(&mut self) {
        if self.input_labels.len() < self.next_slot {
            self.input_labels.resize(self.next_slot, None);
        }
    }

    // ── Public helpers ────────────────────────────────────────────────────────

    /// Returns a fresh WireId that won't collide with LIR-assigned wires or
    /// other temp wires.
    fn alloc_temp_wire(&mut self) -> WireId {
        let id = self.next_temp_wire;
        self.next_temp_wire += 1;
        WireId(id)
    }

    /// Base slot for `wire` (used by benchmark to build eval_base_slots).
    pub fn wire_base_slot(&self, wire: WireId) -> usize {
        self.wire_map[&wire]
    }

    pub fn bit_width(&self) -> usize {
        self.bit_width
    }

    // ── Wire initialisation ───────────────────────────────────────────────────

    fn init_wire(&mut self, wire: WireId) {
        self.alloc_wire(wire); // idempotent; allocates + creates labels
    }

    // ── Circuit-input helpers ─────────────────────────────────────────────────

    /// Create circuit-input slots for a compile-time constant and select the
    /// active label for each bit.  Returns the allocated WireId.
    fn load_constant(&mut self, value: u64) -> WireId {
        let wire = self.alloc_temp_wire();
        let base = self.alloc_wire(wire);
        self.sync_input_labels();
        for b in 0..self.bit_width {
            let bit = ((value >> b) & 1) as u8;
            let slot = base + b;
            self.circuit.add_input_slot(slot);
            if let Some(label) = self.circuit.get_label(slot, bit) {
                self.input_labels[slot] = Some(label);
            }
        }
        wire
    }

    // ── Arithmetic/logic circuit builders ────────────────────────────────────

    fn build_add_constant(&mut self, input: WireId, constant: u64, output: WireId) {
        let c = self.load_constant(constant);
        self.build_add(input, c, output);
    }

    fn build_sub_constant(&mut self, input: WireId, constant: u64, output: WireId) {
        let c = self.load_constant(constant);
        self.build_sub(input, c, output);
    }

    fn build_mul_constant(&mut self, input: WireId, constant: u64, output: WireId) {
        let c = self.load_constant(constant);
        self.build_mul(input, c, output);
    }

    /// N-bit multiplexer: if sel[0] == 1 then out = a, else out = b.
    fn build_mux(&mut self, sel: WireId, a: WireId, b: WireId, out: WireId) {
        self.init_wire(sel);
        self.init_wire(a);
        self.init_wire(b);
        self.init_wire(out);
        let sel_b0 = self.wire_slot(sel, 0);
        for i in 0..self.bit_width {
            let ai = self.wire_slot(a, i);
            let bi = self.wire_slot(b, i);
            let oi = self.wire_slot(out, i);
            let diff   = self.alloc_single_slot();
            let masked = self.alloc_single_slot();
            // diff   = a XOR b
            // masked = sel AND diff
            // out    = masked XOR b   → a when sel=1, b when sel=0
            self.circuit.add_gate(xor_logic(), &[ai, bi], diff);
            self.circuit.add_gate(and_logic(), &[sel_b0, diff], masked);
            self.circuit.add_gate(xor_logic(), &[masked, bi], oi);
            self.circuit.add_output_slot(oi);
        }
    }

    /// Unsigned N-bit restoring division: quotient = dividend / divisor.
    fn build_div(&mut self, dividend: WireId, divisor: WireId, quotient: WireId) {
        let n = self.bit_width;
        self.init_wire(dividend);
        self.init_wire(divisor);
        self.init_wire(quotient);

        let mut rem = self.load_constant(0);

        for step in 0..n {
            let bit_i = n - 1 - step; // MSB first → LSB last

            // Shift rem left by 1, insert dividend[bit_i] at bit 0.
            let shifted = self.alloc_temp_wire();
            self.init_wire(shifted);

            let div_bit = self.wire_slot(dividend, bit_i);
            let s0 = self.wire_slot(shifted, 0);
            self.circuit.add_gate(and_logic(), &[div_bit, div_bit], s0); // copy

            for j in 1..n {
                let rem_bit = self.wire_slot(rem, j - 1);
                let sj = self.wire_slot(shifted, j);
                self.circuit.add_gate(and_logic(), &[rem_bit, rem_bit], sj); // copy
            }

            let candidate = self.alloc_temp_wire();
            self.init_wire(candidate);
            self.build_sub(shifted, divisor, candidate);

            let borrow = self.alloc_temp_wire();
            self.build_less_than(shifted, divisor, borrow);

            let q_bit   = self.wire_slot(quotient, bit_i);
            let borrow0 = self.wire_slot(borrow, 0);
            self.circuit.add_gate(not_logic(), &[borrow0], q_bit);
            self.circuit.add_output_slot(q_bit);

            let new_rem = self.alloc_temp_wire();
            self.build_mux(borrow, shifted, candidate, new_rem);
            rem = new_rem;
        }
    }

    /// Unsigned N-bit modulo: remainder = dividend % divisor.
    fn build_mod(&mut self, dividend: WireId, divisor: WireId, remainder: WireId) {
        let quotient = self.alloc_temp_wire();
        self.init_wire(quotient);
        self.build_div(dividend, divisor, quotient);

        let product = self.alloc_temp_wire();
        self.init_wire(product);
        self.build_mul(quotient, divisor, product);

        self.build_sub(dividend, product, remainder);
    }

    fn build_and(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);
        for b in 0..self.bit_width {
            let s1 = self.wire_slot(in1, b);
            let s2 = self.wire_slot(in2, b);
            let so = self.wire_slot(out, b);
            self.circuit.add_gate(and_logic(), &[s1, s2], so);
            self.circuit.add_output_slot(so);
        }
    }

    fn build_xor(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);
        for b in 0..self.bit_width {
            let s1 = self.wire_slot(in1, b);
            let s2 = self.wire_slot(in2, b);
            let so = self.wire_slot(out, b);
            self.circuit.add_gate(xor_logic(), &[s1, s2], so);
            self.circuit.add_output_slot(so);
        }
    }

    fn build_or(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);
        for b in 0..self.bit_width {
            let s1 = self.wire_slot(in1, b);
            let s2 = self.wire_slot(in2, b);
            let so = self.wire_slot(out, b);
            self.circuit.add_gate(or_logic(), &[s1, s2], so);
            self.circuit.add_output_slot(so);
        }
    }

    fn build_not(&mut self, input: WireId, out: WireId) {
        self.init_wire(input);
        self.init_wire(out);
        for b in 0..self.bit_width {
            let si = self.wire_slot(input, b);
            let so = self.wire_slot(out, b);
            self.circuit.add_gate(not_logic(), &[si], so);
            self.circuit.add_output_slot(so);
        }
    }

    fn build_add(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);
        self.build_add_internal(in1, in2, out);
    }

    fn build_sub(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        // ~b
        let not_in2 = self.alloc_temp_wire();
        self.init_wire(not_in2);
        for b in 0..self.bit_width {
            let sb = self.wire_slot(in2, b);
            let snb = self.wire_slot(not_in2, b);
            self.circuit.add_gate(not_logic(), &[sb], snb);
        }

        // ~b + 1  (two's complement)
        let neg_in2 = self.alloc_temp_wire();
        self.init_wire(neg_in2);

        let not_b0 = self.wire_slot(not_in2, 0);
        let neg_b0 = self.wire_slot(neg_in2, 0);
        self.circuit.add_gate(not_logic(), &[not_b0], neg_b0);

        let mut carry = self.alloc_single_slot();
        self.circuit.add_gate(and_logic(), &[not_b0, not_b0], carry); // carry₀ = ~b0 AND ~b0

        for i in 1..self.bit_width {
            let not_b = self.wire_slot(not_in2, i);
            let neg_b = self.wire_slot(neg_in2, i);
            let cin = carry;
            let cout = self.alloc_single_slot();
            self.circuit.add_gate(xor_logic(), &[not_b, cin], neg_b);
            self.circuit.add_gate(and_logic(), &[not_b, cin], cout);
            carry = cout;
        }

        self.build_add_internal(in1, neg_in2, out);
    }

    /// Unsigned N-bit less-than comparator (MSB-to-LSB ripple).
    fn build_less_than(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        let n = self.bit_width;
        let mut lt_prev: Option<usize> = None;
        let mut eq_prev: Option<usize> = None;

        for step in 0..n {
            let i = n - 1 - step; // MSB first
            let a = self.wire_slot(in1, i);
            let b = self.wire_slot(in2, i);

            let not_a   = self.alloc_single_slot();
            let xor_ab  = self.alloc_single_slot();
            let not_xor = self.alloc_single_slot();
            let less_here = self.alloc_single_slot();

            self.circuit.add_gate(not_logic(), &[a], not_a);
            self.circuit.add_gate(xor_logic(), &[a, b], xor_ab);
            self.circuit.add_gate(not_logic(), &[xor_ab], not_xor);
            self.circuit.add_gate(and_logic(), &[not_a, b], less_here);

            let lt_cur = self.alloc_single_slot();
            let eq_cur = self.alloc_single_slot();

            match (lt_prev, eq_prev) {
                (None, None) => {
                    // MSB: copy directly.
                    self.circuit.add_gate(and_logic(), &[less_here, less_here], lt_cur);
                    self.circuit.add_gate(and_logic(), &[not_xor, not_xor], eq_cur);
                }
                (Some(lp), Some(ep)) => {
                    let lt_contrib = self.alloc_single_slot();
                    self.circuit.add_gate(and_logic(), &[ep, less_here], lt_contrib);
                    self.circuit.add_gate(or_logic(), &[lp, lt_contrib], lt_cur);
                    self.circuit.add_gate(and_logic(), &[ep, not_xor], eq_cur);
                }
                _ => unreachable!(),
            }

            lt_prev = Some(lt_cur);
            eq_prev = Some(eq_cur);
        }

        // Map final lt to output bit 0.
        let out0 = self.wire_slot(out, 0);
        let final_lt = lt_prev.unwrap();
        self.circuit.add_gate(and_logic(), &[final_lt, final_lt], out0);
        self.circuit.add_output_slot(out0);

        // Zero-fill bits 1..n.
        let in1_b0   = self.wire_slot(in1, 0);
        let not_b0   = self.alloc_single_slot();
        self.circuit.add_gate(not_logic(), &[in1_b0], not_b0);
        for bit_idx in 1..n {
            let out_i = self.wire_slot(out, bit_idx);
            self.circuit.add_gate(and_logic(), &[in1_b0, not_b0], out_i);
            self.circuit.add_output_slot(out_i);
        }
    }

    /// N-bit equality check: output bit 0 = 1 iff `in1 == in2`.
    fn build_equal(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        let n = self.bit_width;
        let mut acc: Option<usize> = None;

        for i in 0..n {
            let a = self.wire_slot(in1, i);
            let b = self.wire_slot(in2, i);

            let xor_i     = self.alloc_single_slot();
            let not_xor_i = self.alloc_single_slot();

            self.circuit.add_gate(xor_logic(), &[a, b], xor_i);
            self.circuit.add_gate(not_logic(), &[xor_i], not_xor_i);

            acc = Some(match acc {
                None => {
                    // First bit: copy.
                    let first = self.alloc_single_slot();
                    self.circuit.add_gate(and_logic(), &[not_xor_i, not_xor_i], first);
                    first
                }
                Some(prev) => {
                    let next = self.alloc_single_slot();
                    self.circuit.add_gate(and_logic(), &[prev, not_xor_i], next);
                    next
                }
            });
        }

        // Map to output bit 0.
        let out0 = self.wire_slot(out, 0);
        let final_acc = acc.unwrap();
        self.circuit.add_gate(and_logic(), &[final_acc, final_acc], out0);
        self.circuit.add_output_slot(out0);

        // Zero-fill bits 1..n.
        let in1_b0 = self.wire_slot(in1, 0);
        let not_b0 = self.alloc_single_slot();
        self.circuit.add_gate(not_logic(), &[in1_b0], not_b0);
        for bit_idx in 1..n {
            let out_i = self.wire_slot(out, bit_idx);
            self.circuit.add_gate(and_logic(), &[in1_b0, not_b0], out_i);
            self.circuit.add_output_slot(out_i);
        }
    }

    /// N-bit adder (assumes wires already initialised via alloc_wire).
    fn build_add_internal(&mut self, in1: WireId, in2: WireId, out: WireId) {
        let a0   = self.wire_slot(in1, 0);
        let b0   = self.wire_slot(in2, 0);
        let sum0 = self.wire_slot(out, 0);
        let mut carry = self.alloc_single_slot();

        self.circuit.add_gate(xor_logic(), &[a0, b0], sum0);
        self.circuit.add_gate(and_logic(), &[a0, b0], carry);
        self.circuit.add_output_slot(sum0);

        for i in 1..self.bit_width {
            let a   = self.wire_slot(in1, i);
            let b   = self.wire_slot(in2, i);
            let sum = self.wire_slot(out, i);
            let cin = carry;
            let cout     = self.alloc_single_slot();
            let a_xor_b  = self.alloc_single_slot();
            let a_and_b  = self.alloc_single_slot();
            let cin_axorb = self.alloc_single_slot();

            self.circuit.add_gate(xor_logic(), &[a, b], a_xor_b);
            self.circuit.add_gate(xor_logic(), &[a_xor_b, cin], sum);
            self.circuit.add_gate(and_logic(), &[a, b], a_and_b);
            self.circuit.add_gate(and_logic(), &[cin, a_xor_b], cin_axorb);
            self.circuit.add_gate(or_logic(), &[a_and_b, cin_axorb], cout);
            self.circuit.add_output_slot(sum);
            carry = cout;
        }
    }

    fn build_mul(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        let pp_wires: Vec<WireId> = (0..self.bit_width)
            .map(|_| {
                let w = self.alloc_temp_wire();
                self.init_wire(w);
                w
            })
            .collect();
        let mut partial_products = Vec::new();

        for j in 0..self.bit_width {
            let pp_wire = pp_wires[j];
            let b_bit = self.wire_slot(in2, j);
            for i in 0..self.bit_width {
                let a_bit = self.wire_slot(in1, i);
                let pp_bit = self.wire_slot(pp_wire, i);
                self.circuit.add_gate(and_logic(), &[a_bit, b_bit], pp_bit);
            }
            partial_products.push(pp_wire);
        }

        if partial_products.is_empty() {
            // Result is 0: output XOR a with itself.
            let temp = self.wire_slot(in1, 0);
            for b in 0..self.bit_width {
                let zero = self.alloc_single_slot();
                let out_b = self.wire_slot(out, b);
                self.circuit.add_gate(xor_logic(), &[temp, temp], zero);
                self.circuit.add_gate(xor_logic(), &[zero, zero], out_b);
                self.circuit.add_output_slot(out_b);
            }
            return;
        }

        let mut accumulator = partial_products[0];

        for j in 1..partial_products.len() {
            let shifted_pp = self.alloc_temp_wire();
            self.init_wire(shifted_pp);
            let temp = self.wire_slot(in1, 0);

            for i in 0..self.bit_width {
                let shifted_bit = self.wire_slot(shifted_pp, i);
                if i < j {
                    // zero fill
                    let zero = self.alloc_single_slot();
                    self.circuit.add_gate(xor_logic(), &[temp, temp], zero);
                    self.circuit.add_gate(xor_logic(), &[zero, zero], shifted_bit);
                } else if i - j < self.bit_width {
                    let pp_bit = self.wire_slot(partial_products[j], i - j);
                    let zero = self.alloc_single_slot();
                    self.circuit.add_gate(xor_logic(), &[temp, temp], zero);
                    self.circuit.add_gate(xor_logic(), &[pp_bit, zero], shifted_bit);
                }
            }

            let new_acc = self.alloc_temp_wire();
            self.init_wire(new_acc);
            self.build_add_internal(accumulator, shifted_pp, new_acc);
            accumulator = new_acc;
        }

        let temp = self.wire_slot(in1, 0);
        for b in 0..self.bit_width {
            let acc_bit = self.wire_slot(accumulator, b);
            let out_bit = self.wire_slot(out, b);
            let zero = self.alloc_single_slot();
            self.circuit.add_gate(xor_logic(), &[temp, temp], zero);
            self.circuit.add_gate(xor_logic(), &[acc_bit, zero], out_bit);
            self.circuit.add_output_slot(out_bit);
        }
    }

    // ── Public API for garbling / OT ─────────────────────────────────────────

    /// Return the label pair `[label₀, label₁]` for one bit of a wire.
    pub fn wire_label_pair(&self, wire: WireId, bit: usize) -> Option<[u128; 2]> {
        self.circuit.labels.get(self.wire_slot(wire, bit)).copied()
    }

    /// Register an evaluator-owned wire as a circuit input.
    /// Labels are created; active labels will arrive via OT.
    pub fn register_evaluator_wire(&mut self, wire: WireId) {
        let base = self.alloc_wire(wire);
        for b in 0..self.bit_width {
            self.circuit.add_input_slot(base + b);
        }
    }

    /// Assign active input labels for `wire` from plaintext `value` (no OT).
    pub fn assign_input_labels(&mut self, wire: WireId, value: u64) {
        let base = self.alloc_wire(wire);
        self.sync_input_labels();
        for b in 0..self.bit_width {
            let bit = ((value >> b) & 1) as u8;
            let slot = base + b;
            self.circuit.add_input_slot(slot);
            if let Some(label) = self.circuit.get_label(slot, bit) {
                self.input_labels[slot] = Some(label);
            }
        }
    }

    /// Garble the circuit and return what the evaluator needs:
    /// - The stripped `Circuit` (garbled tables, no label pairs).
    /// - `active`: flat Vec of active input labels indexed by slot.
    /// - `decode`: flat Vec indexed by slot; `decode[slot] = lsb(label₀)`
    ///    for output slots (used to recover plaintext bits).
    pub fn finalize_garbler(
        &mut self,
    ) -> (Circuit, Vec<Option<u128>>, Vec<u8>) {
        self.circuit.garble();
        let n = self.circuit.n_slots;
        let mut decode = vec![0u8; n];
        for &slot in &self.circuit.outputs {
            if slot < self.circuit.labels.len() {
                decode[slot] = (self.circuit.labels[slot][0] & 1) as u8;
            }
        }
        let mut eval_circuit = self.circuit.clone();
        eval_circuit.labels = Vec::new(); // strip label pairs
        self.sync_input_labels();
        (eval_circuit, self.input_labels.clone(), decode)
    }

    fn evaluate_circuit(&mut self) -> Result<(), BackendError> {
        if self.garbled {
            return Ok(());
        }

        self.circuit.garble();

        // Prepare active labels Vec.
        self.sync_input_labels();
        let active_in = self.input_labels.clone();

        // Use parallel evaluation for circuits up to ~500K gates.
        let results = if self.circuit.gates.len() <= 500_000 {
            self.circuit.evaluate_parallel(active_in)
        } else {
            self.circuit.evaluate(active_in)
        };

        // Decode outputs: iterate over wire_map to find which wires have
        // output slots, then reconstruct the u64 value for each.
        let outputs_set: std::collections::HashSet<usize> =
            self.circuit.outputs.iter().copied().collect();

        for (&wire, &base) in &self.wire_map {
            // Check if any bit of this wire is an output.
            let is_output = (0..self.bit_width).any(|b| outputs_set.contains(&(base + b)));
            if !is_output {
                continue;
            }
            let mut value = 0u64;
            for b in 0..self.bit_width {
                let slot = base + b;
                if let Some(active_lbl) = results.get(slot).and_then(|x| *x) {
                    let decode_bit = if slot < self.circuit.labels.len() {
                        self.circuit.labels[slot][0] & 1
                    } else {
                        0
                    };
                    let bit = ((active_lbl & 1) ^ decode_bit) as u64;
                    value |= bit << b;
                }
            }
            self.evaluation_cache.insert(wire, value);
        }

        self.garbled = true;
        Ok(())
    }
}

impl Backend for YaoBackend {
    fn name(&self) -> &'static str {
        "Yao Garbled Circuits"
    }

    fn set_input(
        &mut self,
        wire: WireId,
        value: u64,
        visibility: Visibility,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        let base = self.alloc_wire(wire);
        self.sync_input_labels();
        for b in 0..self.bit_width {
            let bit = ((value >> b) & 1) as u8;
            let slot = base + b;
            self.circuit.add_input_slot(slot);
            if let Some(label) = self.circuit.get_label(slot, bit) {
                self.input_labels[slot] = Some(label);
            }
        }
        state.set_wire(wire, WireValue::Secret, visibility);
        Ok(())
    }

    fn get_output(&mut self, wire: WireId, _state: &VMState) -> Result<u64, BackendError> {
        self.evaluate_circuit()?;
        self.evaluation_cache
            .get(&wire)
            .copied()
            .ok_or(BackendError::WireNotSet(wire))
    }

    fn execute_instruction(
        &mut self,
        instruction: &Instruction,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        match instruction {
            Instruction::And { vis, input1, input2, output } => {
                self.build_and(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::Xor { vis, input1, input2, output } => {
                self.build_xor(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::Not { vis, input, output } => {
                self.build_not(*input, *output);
                state.set_wire(*output, WireValue::Secret, *vis);
                Ok(())
            }
            Instruction::Or { vis, input1, input2, output } => {
                self.build_or(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::Add { vis, input1, input2, output, .. } => {
                self.build_add(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::Sub { vis, input1, input2, output, .. } => {
                self.build_sub(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::Mul { vis, input1, input2, output, .. } => {
                self.build_mul(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::Constant { value, output, visibility, .. } => {
                let base = self.alloc_wire(*output);
                self.sync_input_labels();
                for b in 0..self.bit_width {
                    let bit = ((*value >> b) & 1) as u8;
                    let slot = base + b;
                    self.circuit.add_input_slot(slot);
                    if let Some(label) = self.circuit.get_label(slot, bit) {
                        self.input_labels[slot] = Some(label);
                    }
                }
                state.set_wire(*output, WireValue::Secret, *visibility);
                Ok(())
            }
            Instruction::LessThan { vis, input1, input2, output } => {
                self.build_less_than(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::Equal { vis, input1, input2, output } => {
                self.build_equal(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::AddConstant { vis, input, constant, output, .. } => {
                self.build_add_constant(*input, *constant, *output);
                state.set_wire(*output, WireValue::Secret, *vis);
                Ok(())
            }
            Instruction::SubConstant { vis, input, constant, output, .. } => {
                self.build_sub_constant(*input, *constant, *output);
                state.set_wire(*output, WireValue::Secret, *vis);
                Ok(())
            }
            Instruction::MulConstant { vis, input, constant, output, .. } => {
                self.build_mul_constant(*input, *constant, *output);
                state.set_wire(*output, WireValue::Secret, *vis);
                Ok(())
            }
            Instruction::Div { vis, input1, input2, output, .. } => {
                self.build_div(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::Mod { vis, input1, input2, output, .. } => {
                self.build_mod(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }
            Instruction::Select { output_vis, condition, then_val, else_val, output } => {
                self.build_mux(*condition, *then_val, *else_val, *output);
                state.set_wire(*output, WireValue::Secret, *output_vis);
                Ok(())
            }
        }
    }
}
