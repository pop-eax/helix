use crate::circuit::Circuit;
use crate::gate::{and_logic, not_logic, or_logic, xor_logic};
use ir::lir::WireId;
use runtime::vm::{Backend, BackendError, Instruction, VMState, WireValue};
use runtime::Visibility;
use std::collections::HashMap;

pub struct YaoBackend {
    circuit: Circuit,
    bit_width: usize,
    initialized_wires: HashMap<WireId, bool>,
    input_labels: HashMap<String, u128>,
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
            initialized_wires: HashMap::new(),
            input_labels: HashMap::new(),
            garbled: false,
            evaluation_cache: HashMap::new(),
            next_temp_wire: 1_000_000,
        }
    }

    /// Returns a fresh WireId that won't collide with LIR-assigned wires or
    /// other temp wires.
    fn alloc_temp_wire(&mut self) -> WireId {
        let id = self.next_temp_wire;
        self.next_temp_wire += 1;
        WireId(id)
    }

    /// Creates circuit-input wires for a compile-time constant and registers the
    /// correct labels so the garbler always provides the right bit values.
    /// Returns the allocated WireId (already marked as initialized).
    fn load_constant(&mut self, value: u64) -> WireId {
        let wire = self.alloc_temp_wire();
        for bit_idx in 0..self.bit_width {
            let bit = ((value >> bit_idx) & 1) as u8;
            let wire_name = self.wire_bit_name(wire, bit_idx);
            self.circuit.add_input(&wire_name);
            if let Some(label) = self.circuit.get_label(&wire_name, bit) {
                self.input_labels.insert(wire_name, label);
            }
        }
        self.initialized_wires.insert(wire, true);
        wire
    }

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
    /// sel is treated as a 1-bit signal (only bit 0 is used).
    fn build_mux(&mut self, sel: WireId, a: WireId, b: WireId, out: WireId) {
        self.init_wire(sel);
        self.init_wire(a);
        self.init_wire(b);
        self.init_wire(out);
        let sel_b0 = self.wire_bit_name(sel, 0);
        for i in 0..self.bit_width {
            let ai = self.wire_bit_name(a, i);
            let bi = self.wire_bit_name(b, i);
            let oi = self.wire_bit_name(out, i);
            let diff   = format!("mux_diff_{}_{}_{}_{}_{}", sel.0, a.0, b.0, out.0, i);
            let masked = format!("mux_mask_{}_{}_{}_{}_{}", sel.0, a.0, b.0, out.0, i);
            // diff   = a XOR b
            // masked = sel AND diff
            // out    = masked XOR b   → a when sel=1, b when sel=0
            self.circuit.add_gate(xor_logic(), &[&ai, &bi], &diff);
            self.circuit.add_gate(and_logic(), &[&sel_b0, &diff], &masked);
            self.circuit.add_gate(xor_logic(), &[&masked, &bi], &oi);
            self.circuit.add_output(&oi);
        }
    }

    /// Unsigned N-bit restoring division: quotient = dividend / divisor.
    /// Processes bits MSB-first; each iteration shifts the partial remainder
    /// left by one, inserts the next dividend bit, and conditionally subtracts
    /// the divisor.
    fn build_div(&mut self, dividend: WireId, divisor: WireId, quotient: WireId) {
        let n = self.bit_width;
        self.init_wire(dividend);
        self.init_wire(divisor);
        self.init_wire(quotient);

        // Start with partial remainder = 0.
        let mut rem = self.load_constant(0);

        for step in 0..n {
            let bit_i = n - 1 - step; // MSB first → LSB last

            // --- Shift rem left by 1, insert dividend[bit_i] at bit 0 ---
            let shifted = self.alloc_temp_wire();
            self.init_wire(shifted);

            let div_bit = self.wire_bit_name(dividend, bit_i);
            let s0 = self.wire_bit_name(shifted, 0);
            self.circuit.add_gate(and_logic(), &[&div_bit, &div_bit], &s0); // copy

            for j in 1..n {
                let rem_bit = self.wire_bit_name(rem, j - 1);
                let sj = self.wire_bit_name(shifted, j);
                self.circuit.add_gate(and_logic(), &[&rem_bit, &rem_bit], &sj); // copy
            }

            // --- candidate = shifted - divisor ---
            let candidate = self.alloc_temp_wire();
            self.init_wire(candidate);
            self.build_sub(shifted, divisor, candidate);

            // --- borrow = (shifted < divisor) ---
            let borrow = self.alloc_temp_wire();
            self.build_less_than(shifted, divisor, borrow);

            // --- quotient[bit_i] = NOT(borrow[0]) ---
            let q_bit   = self.wire_bit_name(quotient, bit_i);
            let borrow0 = self.wire_bit_name(borrow, 0);
            self.circuit.add_gate(not_logic(), &[&borrow0], &q_bit);
            self.circuit.add_output(&q_bit);

            // --- new_rem = borrow ? shifted : candidate ---
            let new_rem = self.alloc_temp_wire();
            self.build_mux(borrow, shifted, candidate, new_rem);
            rem = new_rem;
        }
    }

    /// Unsigned N-bit modulo: remainder = dividend % divisor.
    fn build_mod(&mut self, dividend: WireId, divisor: WireId, remainder: WireId) {
        // remainder = dividend - quotient * divisor
        let quotient = self.alloc_temp_wire();
        self.init_wire(quotient);
        self.build_div(dividend, divisor, quotient);

        let product = self.alloc_temp_wire();
        self.init_wire(product);
        self.build_mul(quotient, divisor, product);

        self.build_sub(dividend, product, remainder);
    }

    fn wire_bit_name(&self, wire: WireId, bit_idx: usize) -> String {
        format!("w{}_b{}", wire.0, bit_idx)
    }

    fn init_wire(&mut self, wire: WireId) {
        if !self.initialized_wires.contains_key(&wire) {
            for bit_idx in 0..self.bit_width {
                let wire_name = self.wire_bit_name(wire, bit_idx);
                self.circuit.get_or_create_labels(&wire_name);
            }
            self.initialized_wires.insert(wire, true);
        }
    }

    fn build_and(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        for bit_idx in 0..self.bit_width {
            let w1 = self.wire_bit_name(in1, bit_idx);
            let w2 = self.wire_bit_name(in2, bit_idx);
            let wo = self.wire_bit_name(out, bit_idx);

            self.circuit.add_gate(and_logic(), &[&w1, &w2], &wo);
            self.circuit.add_output(&wo);
        }
    }

    fn build_xor(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        for bit_idx in 0..self.bit_width {
            let w1 = self.wire_bit_name(in1, bit_idx);
            let w2 = self.wire_bit_name(in2, bit_idx);
            let wo = self.wire_bit_name(out, bit_idx);

            self.circuit.add_gate(xor_logic(), &[&w1, &w2], &wo);
            self.circuit.add_output(&wo);
        }
    }

    fn build_or(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        for bit_idx in 0..self.bit_width {
            let w1 = self.wire_bit_name(in1, bit_idx);
            let w2 = self.wire_bit_name(in2, bit_idx);
            let wo = self.wire_bit_name(out, bit_idx);

            self.circuit.add_gate(or_logic(), &[&w1, &w2], &wo);
            self.circuit.add_output(&wo);
        }
    }

    fn build_not(&mut self, input: WireId, out: WireId) {
        self.init_wire(input);
        self.init_wire(out);

        for bit_idx in 0..self.bit_width {
            let wi = self.wire_bit_name(input, bit_idx);
            let wo = self.wire_bit_name(out, bit_idx);

            self.circuit.add_gate(not_logic(), &[&wi], &wo);
            self.circuit.add_output(&wo);
        }
    }

    fn build_add(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        // Bit 0: half adder
        let a0 = self.wire_bit_name(in1, 0);
        let b0 = self.wire_bit_name(in2, 0);
        let sum0 = self.wire_bit_name(out, 0);
        let c0 = format!("carry_{}_{}_0", in1.0, in2.0);

        self.circuit.add_gate(xor_logic(), &[&a0, &b0], &sum0);
        self.circuit.add_gate(and_logic(), &[&a0, &b0], &c0);
        self.circuit.add_output(&sum0);

        // Bits 1..bit_width: full adders
        for i in 1..self.bit_width {
            let a = self.wire_bit_name(in1, i);
            let b = self.wire_bit_name(in2, i);
            let cin = format!("carry_{}_{}_{}", in1.0, in2.0, i - 1);
            let sum = self.wire_bit_name(out, i);
            let cout = format!("carry_{}_{}_{}", in1.0, in2.0, i);

            let a_xor_b = format!("axorb_{}_{}_{}", in1.0, in2.0, i);
            let a_and_b = format!("aandb_{}_{}_{}", in1.0, in2.0, i);
            let cin_and_axorb = format!("cinandb_{}_{}_{}", in1.0, in2.0, i);

            self.circuit.add_gate(xor_logic(), &[&a, &b], &a_xor_b);
            self.circuit.add_gate(xor_logic(), &[&a_xor_b, &cin], &sum);
            self.circuit.add_gate(and_logic(), &[&a, &b], &a_and_b);
            self.circuit
                .add_gate(and_logic(), &[&cin, &a_xor_b], &cin_and_axorb);
            self.circuit
                .add_gate(or_logic(), &[&a_and_b, &cin_and_axorb], &cout);

            self.circuit.add_output(&sum);
        }
    }

    fn build_sub(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        // Step 1: Invert in2 (create ~b). Use alloc_temp_wire so this method
        // is safe to call multiple times with the same in2 (e.g., inside loops).
        let not_in2 = self.alloc_temp_wire();
        self.init_wire(not_in2);

        for bit_idx in 0..self.bit_width {
            let b = self.wire_bit_name(in2, bit_idx);
            let not_b = self.wire_bit_name(not_in2, bit_idx);
            self.circuit.add_gate(not_logic(), &[&b], &not_b);
        }

        // Step 2: Add 1 to ~b to get two's complement negation.
        let neg_in2 = self.alloc_temp_wire();
        self.init_wire(neg_in2);

        let not_b0 = self.wire_bit_name(not_in2, 0);
        let neg_b0 = self.wire_bit_name(neg_in2, 0);
        self.circuit.add_gate(not_logic(), &[&not_b0], &neg_b0);

        let c0 = format!("neg_carry_{}_0", not_in2.0);
        self.circuit.add_gate(and_logic(), &[&not_b0, &not_b0], &c0);

        for i in 1..self.bit_width {
            let not_b = self.wire_bit_name(not_in2, i);
            let neg_b = self.wire_bit_name(neg_in2, i);
            let cin  = format!("neg_carry_{}_{}", not_in2.0, i - 1);
            let cout = format!("neg_carry_{}_{}", not_in2.0, i);
            self.circuit.add_gate(xor_logic(), &[&not_b, &cin], &neg_b);
            self.circuit.add_gate(and_logic(), &[&not_b, &cin], &cout);
        }

        // Step 3: in1 + (-in2)
        self.build_add_internal(in1, neg_in2, out);
    }

    /// Unsigned N-bit less-than comparator (MSB-to-LSB ripple).
    ///
    /// Produces 1 in bit 0 of `out` iff `in1 < in2` (unsigned).
    /// All other output bits are unused (not added as circuit outputs).
    fn build_less_than(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        let n = self.bit_width;

        // Process bits from MSB (n-1) down to 0, propagating lt/eq state.
        // lt_wire: "in1[MSB..i+1] < in2[MSB..i+1]"
        // eq_wire: "in1[MSB..i+1] == in2[MSB..i+1]"
        let mut lt_prev: Option<String> = None;
        let mut eq_prev: Option<String> = None;

        for step in 0..n {
            let i = n - 1 - step; // MSB first
            let a = self.wire_bit_name(in1, i);
            let b = self.wire_bit_name(in2, i);

            // not_a = NOT(a[i])
            let not_a = format!("cmp_nota_{}_{}_{}", in1.0, in2.0, i);
            self.circuit.add_gate(not_logic(), &[&a], &not_a);

            // xor_ab = a[i] XOR b[i]   ("bits differ")
            let xor_ab = format!("cmp_xor_{}_{}_{}", in1.0, in2.0, i);
            self.circuit.add_gate(xor_logic(), &[&a, &b], &xor_ab);

            // not_xor = NOT(xor_ab)   ("bits equal")
            let not_xor = format!("cmp_nxor_{}_{}_{}", in1.0, in2.0, i);
            self.circuit.add_gate(not_logic(), &[&xor_ab], &not_xor);

            // less_here = not_a AND b[i]   ("a=0, b=1 at this bit")
            let less_here = format!("cmp_lh_{}_{}_{}", in1.0, in2.0, i);
            self.circuit.add_gate(and_logic(), &[&not_a, &b], &less_here);

            let lt_cur = format!("cmp_lt_{}_{}_{}", in1.0, in2.0, i);
            let eq_cur = format!("cmp_eq_{}_{}_{}", in1.0, in2.0, i);

            match (&lt_prev, &eq_prev) {
                (None, None) => {
                    // MSB: initialise state directly from this bit.
                    self.circuit.add_gate(and_logic(), &[&less_here, &less_here], &lt_cur); // copy
                    self.circuit.add_gate(and_logic(), &[&not_xor, &not_xor], &eq_cur);   // copy
                }
                (Some(lp), Some(ep)) => {
                    // lt_contrib = eq_prev AND less_here
                    let lt_contrib = format!("cmp_lc_{}_{}_{}", in1.0, in2.0, i);
                    self.circuit.add_gate(and_logic(), &[ep, &less_here], &lt_contrib);

                    // lt_cur = lt_prev OR lt_contrib
                    self.circuit.add_gate(or_logic(), &[lp, &lt_contrib], &lt_cur);

                    // eq_cur = eq_prev AND not_xor
                    self.circuit.add_gate(and_logic(), &[ep, &not_xor], &eq_cur);
                }
                _ => unreachable!(),
            }

            lt_prev = Some(lt_cur);
            eq_prev = Some(eq_cur);
        }

        // Map final lt result to output bit 0.
        let out0 = self.wire_bit_name(out, 0);
        let final_lt = lt_prev.unwrap();
        self.circuit.add_gate(and_logic(), &[&final_lt, &final_lt], &out0); // copy
        self.circuit.add_output(&out0);
    }

    /// N-bit equality check: output bit 0 = 1 iff `in1 == in2`.
    fn build_equal(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        let n = self.bit_width;
        let mut acc: Option<String> = None;

        for i in 0..n {
            let a = self.wire_bit_name(in1, i);
            let b = self.wire_bit_name(in2, i);

            // xor_i = a[i] XOR b[i]
            let xor_i = format!("eq_xor_{}_{}_{}", in1.0, in2.0, i);
            self.circuit.add_gate(xor_logic(), &[&a, &b], &xor_i);

            // not_xor_i = NOT(xor_i)   ("bit i is equal")
            let not_xor_i = format!("eq_nxor_{}_{}_{}", in1.0, in2.0, i);
            self.circuit.add_gate(not_logic(), &[&xor_i], &not_xor_i);

            acc = Some(match acc {
                None => {
                    // First bit: acc = not_xor_i (copy via AND with self)
                    let first = format!("eq_acc_{}_{}_{}", in1.0, in2.0, i);
                    self.circuit.add_gate(and_logic(), &[&not_xor_i, &not_xor_i], &first);
                    first
                }
                Some(prev) => {
                    let next = format!("eq_acc_{}_{}_{}", in1.0, in2.0, i);
                    self.circuit.add_gate(and_logic(), &[&prev, &not_xor_i], &next);
                    next
                }
            });
        }

        // Map to output bit 0.
        let out0 = self.wire_bit_name(out, 0);
        let final_acc = acc.unwrap();
        self.circuit.add_gate(and_logic(), &[&final_acc, &final_acc], &out0);
        self.circuit.add_output(&out0);
    }

    // Same as build_add but assumes wires are already initialized (I should remove it later and add checks to the regular add function)
    fn build_add_internal(&mut self, in1: WireId, in2: WireId, out: WireId) {
        // Bit 0: half adder
        let a0 = self.wire_bit_name(in1, 0);
        let b0 = self.wire_bit_name(in2, 0);
        let sum0 = self.wire_bit_name(out, 0);
        let c0 = format!("carry_{}_{}_0", in1.0, in2.0);

        self.circuit.add_gate(xor_logic(), &[&a0, &b0], &sum0);
        self.circuit.add_gate(and_logic(), &[&a0, &b0], &c0);
        self.circuit.add_output(&sum0);

        // Bits 1..bit_width: full adders
        for i in 1..self.bit_width {
            let a = self.wire_bit_name(in1, i);
            let b = self.wire_bit_name(in2, i);
            let cin = format!("carry_{}_{}_{}", in1.0, in2.0, i - 1);
            let sum = self.wire_bit_name(out, i);
            let cout = format!("carry_{}_{}_{}", in1.0, in2.0, i);

            let a_xor_b = format!("axorb_{}_{}_{}", in1.0, in2.0, i);
            let a_and_b = format!("aandb_{}_{}_{}", in1.0, in2.0, i);
            let cin_and_axorb = format!("cinandb_{}_{}_{}", in1.0, in2.0, i);

            self.circuit.add_gate(xor_logic(), &[&a, &b], &a_xor_b);
            self.circuit.add_gate(xor_logic(), &[&a_xor_b, &cin], &sum);
            self.circuit.add_gate(and_logic(), &[&a, &b], &a_and_b);
            self.circuit
                .add_gate(and_logic(), &[&cin, &a_xor_b], &cin_and_axorb);
            self.circuit
                .add_gate(or_logic(), &[&a_and_b, &cin_and_axorb], &cout);

            self.circuit.add_output(&sum);
        }
    }

    fn build_mul(&mut self, in1: WireId, in2: WireId, out: WireId) {
        self.init_wire(in1);
        self.init_wire(in2);
        self.init_wire(out);

        // Array multiplier algorithm:
        // For each bit j in in2:
        //   If in2[j] == 1, add (in1 << j) to result

        // Generate partial products
        let pp_wires: Vec<WireId> = (0..self.bit_width).map(|_| self.alloc_temp_wire()).collect();
        let mut partial_products = Vec::new();

        for j in 0..self.bit_width {
            let pp_wire = pp_wires[j];
            self.init_wire(pp_wire);

            let b_bit = self.wire_bit_name(in2, j);

            // Partial product j: AND each bit of in1 with in2[j]
            for i in 0..self.bit_width {
                let a_bit = self.wire_bit_name(in1, i);
                let pp_bit = self.wire_bit_name(pp_wire, i);

                self.circuit
                    .add_gate(and_logic(), &[&a_bit, &b_bit], &pp_bit);
            }

            partial_products.push(pp_wire);
        }

        // Sum all partial products with proper shifting
        // We'll use a tree of adders to sum them up

        if partial_products.is_empty() {
            // Result is 0
            for bit_idx in 0..self.bit_width {
                let out_bit = self.wire_bit_name(out, bit_idx);
                let zero_wire = format!("zero_const_{}_{}", out.0, bit_idx);

                // Create a constant 0 (a XOR a = 0)
                let temp = self.wire_bit_name(in1, 0);
                self.circuit
                    .add_gate(xor_logic(), &[&temp, &temp], &zero_wire);
                self.circuit
                    .add_gate(xor_logic(), &[&zero_wire, &zero_wire], &out_bit);
                self.circuit.add_output(&out_bit);
            }
            return;
        }

        // Start with first partial product (no shift needed)
        let mut accumulator = partial_products[0];

        // Add remaining partial products with shifts
        for j in 1..partial_products.len() {
            let shifted_pp = self.alloc_temp_wire();
            self.init_wire(shifted_pp);

            // Shift partial_products[j] left by j positions
            for i in 0..self.bit_width {
                if i < j {
                    // Lower bits are 0
                    let zero_wire = format!("zero_shift_{}_{}_{}", out.0, j, i);
                    let temp = self.wire_bit_name(in1, 0);
                    self.circuit
                        .add_gate(xor_logic(), &[&temp, &temp], &zero_wire);

                    let shifted_bit = self.wire_bit_name(shifted_pp, i);
                    self.circuit
                        .add_gate(xor_logic(), &[&zero_wire, &zero_wire], &shifted_bit);
                } else if i - j < self.bit_width {
                    // Copy from pp[i-j]
                    let pp_bit = self.wire_bit_name(partial_products[j], i - j);
                    let shifted_bit = self.wire_bit_name(shifted_pp, i);

                    // Copy using XOR with 0: a XOR 0 = a
                    let zero_wire = format!("zero_copy_{}_{}_{}", out.0, j, i);
                    let temp = self.wire_bit_name(in1, 0);
                    self.circuit
                        .add_gate(xor_logic(), &[&temp, &temp], &zero_wire);
                    self.circuit
                        .add_gate(xor_logic(), &[&pp_bit, &zero_wire], &shifted_bit);
                }
            }

            // Add to accumulator
            let new_acc = self.alloc_temp_wire();
            self.init_wire(new_acc);
            self.build_add_internal(accumulator, shifted_pp, new_acc);
            accumulator = new_acc;
        }

        // Copy accumulator to output (only lower bits, multiplication can overflow)
        for bit_idx in 0..self.bit_width {
            let acc_bit = self.wire_bit_name(accumulator, bit_idx);
            let out_bit = self.wire_bit_name(out, bit_idx);

            // Copy using identity: a XOR 0 = a
            let zero_wire = format!("zero_final_{}_{}", out.0, bit_idx);
            let temp = self.wire_bit_name(in1, 0);
            self.circuit
                .add_gate(xor_logic(), &[&temp, &temp], &zero_wire);
            self.circuit
                .add_gate(xor_logic(), &[&acc_bit, &zero_wire], &out_bit);

            self.circuit.add_output(&out_bit);
        }
    }

    /// Return the label pair `[label₀, label₁]` for one bit of a wire.
    ///
    /// Used by the garbler to supply OT messages for evaluator-owned wires.
    /// Returns `None` if the wire/bit has not been initialised yet.
    pub fn wire_label_pair(&self, wire: WireId, bit_idx: usize) -> Option<[u128; 2]> {
        let name = self.wire_bit_name(wire, bit_idx);
        self.circuit.labels.get(&name).copied()
    }

    /// Register an evaluator-owned wire as a circuit input (creates its labels
    /// without selecting active labels — those come from OT).
    pub fn register_evaluator_wire(&mut self, wire: WireId) {
        for bit_idx in 0..self.bit_width {
            let wire_name = self.wire_bit_name(wire, bit_idx);
            self.circuit.get_or_create_labels(&wire_name);
            self.circuit.add_input(&wire_name);
        }
    }

    pub fn bit_width(&self) -> usize {
        self.bit_width
    }

    /// Assign active input labels for `wire` based on the plaintext `value`.
    ///
    /// Used by the garbler to inject the evaluator's input labels (received
    /// in plaintext, no OT) without touching `VMState`.
    pub fn assign_input_labels(&mut self, wire: WireId, value: u64) {
        self.init_wire(wire);
        for bit_idx in 0..self.bit_width {
            let bit = ((value >> bit_idx) & 1) as u8;
            let wire_name = self.wire_bit_name(wire, bit_idx);
            self.circuit.add_input(&wire_name);
            if let Some(label) = self.circuit.get_label(&wire_name, bit) {
                self.input_labels.insert(wire_name, label);
            }
        }
    }

    /// Garble the circuit and return everything the evaluator needs:
    ///
    /// - The garbled `Circuit` (gates carry their garbled tables).
    /// - The active input labels (one selected label per input bit wire).
    /// - The output label pairs (both `label[0]` and `label[1]` per output bit
    ///   wire) so the evaluator can decode the final result locally.
    pub fn finalize_garbler(
        &mut self,
    ) -> (Circuit, std::collections::HashMap<String, u128>, std::collections::HashMap<String, u8>) {
        self.circuit.garble();
        // Send only lsb(label₀) per output bit as the decode table.
        // With the color-bit convention (lsb(label₀)=0, lsb(label₁)=1) enforced
        // during label generation, this is always 0 — but we keep it explicit so
        // the evaluator's decoding logic is independent of that internal invariant.
        let decode_table = self
            .circuit
            .outputs
            .iter()
            .filter_map(|name| {
                self.circuit
                    .labels
                    .get(name)
                    .map(|&[l0, _]| (name.clone(), (l0 & 1) as u8))
            })
            .collect();
        (self.circuit.clone(), self.input_labels.clone(), decode_table)
    }

    fn evaluate_circuit(&mut self) -> Result<(), BackendError> {
        if self.garbled {
            return Ok(());
        }

        println!(
            "Garbling circuit with {} gates...",
            self.circuit.gates.len()
        );
        self.circuit.garble();

        // Evaluate
        let results = self.circuit.evaluate(self.input_labels.clone());

        // Decode bit-level results
        let mut processed_wires = std::collections::HashSet::new();

        for output_wire_name in &self.circuit.outputs {
            if let Some((wire_part, _bit_part)) = output_wire_name.split_once("_b") {
                if let Some(id_str) = wire_part.strip_prefix("w") {
                    if let Ok(wire_id) = id_str.parse::<u64>() {
                        let wire = WireId(wire_id as usize);

                        if !processed_wires.contains(&wire) {
                            let mut value = 0u64;

                            for bit_idx in 0..self.bit_width {
                                let bit_wire_name = self.wire_bit_name(wire, bit_idx);

                                // Decode using one bit per output wire:
                                // bit = lsb(active_label) XOR lsb(label₀)
                                if let Some(&output_label) = results.get(&bit_wire_name) {
                                    let decode_bit = self.circuit.labels[&bit_wire_name][0] & 1;
                                    let bit = ((output_label & 1) ^ decode_bit) as u64;
                                    value |= bit << bit_idx;
                                }
                            }

                            self.evaluation_cache.insert(wire, value);
                            processed_wires.insert(wire);
                        }
                    }
                }
            }
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
        self.init_wire(wire);

        // Store the label for each bit
        for bit_idx in 0..self.bit_width {
            let bit = ((value >> bit_idx) & 1) as u8;
            let wire_name = self.wire_bit_name(wire, bit_idx);

            self.circuit.add_input(&wire_name);

            // should use OT here later
            if let Some(label) = self.circuit.get_label(&wire_name, bit) {
                self.input_labels.insert(wire_name, label);
            }
        }

        // Mark in state (but value is Secret)
        state.set_wire(wire, WireValue::Secret, visibility);
        Ok(())
    }

    // NOTE: in the future should fetch this over the network
    fn get_output(&mut self, wire: WireId, _state: &VMState) -> Result<u64, BackendError> {
        // Trigger evaluation if needed
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
            Instruction::And {
                vis,
                input1,
                input2,
                output,
            } => {
                self.build_and(*input1, *input2, *output);
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
                Ok(())
            }

            Instruction::Xor {
                vis,
                input1,
                input2,
                output,
            } => {
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

            Instruction::Add {
                vis,
                input1,
                input2,
                output,
                ..
            } => {
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

            Instruction::Constant {
                value,
                output,
                visibility,
                ..
            } => {
                self.init_wire(*output);

                for bit_idx in 0..self.bit_width {
                    let bit = ((*value >> bit_idx) & 1) as u8;
                    let wire_name = self.wire_bit_name(*output, bit_idx);

                    self.circuit.add_input(&wire_name);

                    if let Some(label) = self.circuit.get_label(&wire_name, bit) {
                        self.input_labels.insert(wire_name, label);
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
        }
    }
}
