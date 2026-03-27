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
        }
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

        // Step 1: Invert in2 (create ~b)
        let not_in2 = WireId(in2.0 + 10000); // Temporary wire for ~b
        self.init_wire(not_in2);

        for bit_idx in 0..self.bit_width {
            let b = self.wire_bit_name(in2, bit_idx);
            let not_b = self.wire_bit_name(not_in2, bit_idx);

            self.circuit.add_gate(not_logic(), &[&b], &not_b);
        }

        // Step 2: Add 1 to ~b (create ~b + 1)
        let neg_in2 = WireId(in2.0 + 20000); // Temporary wire for -b
        self.init_wire(neg_in2);

        // Bit 0: XOR with 1 (flip bit 0)
        let not_b0 = self.wire_bit_name(not_in2, 0);
        let neg_b0 = self.wire_bit_name(neg_in2, 0);

        // not_b[0] XOR 1 = NOT(not_b[0])
        self.circuit.add_gate(not_logic(), &[&not_b0], &neg_b0);

        // Carry for bit 0: AND with 1 = not_b[0]
        let c0 = format!("neg_carry_{}_0", in2.0);
        self.circuit.add_gate(and_logic(), &[&not_b0, &not_b0], &c0); // Effectively just not_b[0]

        // Bits 1..bit_width: Propagate carry
        for i in 1..self.bit_width {
            let not_b = self.wire_bit_name(not_in2, i);
            let neg_b = self.wire_bit_name(neg_in2, i);
            let cin = format!("neg_carry_{}_{}", in2.0, i - 1);
            let cout = format!("neg_carry_{}_{}", in2.0, i);

            // neg_b[i] = not_b[i] XOR carry
            self.circuit.add_gate(xor_logic(), &[&not_b, &cin], &neg_b);

            // carry_out = not_b[i] AND carry
            self.circuit.add_gate(and_logic(), &[&not_b, &cin], &cout);
        }

        // Step 3: Add in1 + (-in2) using existing adder
        self.build_add_internal(in1, neg_in2, out);
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
        let mut partial_products = Vec::new();

        for j in 0..self.bit_width {
            let pp_wire = WireId(out.0 + 30000 + j);
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
                let zero_wire = format!("zero_const_{}", bit_idx);

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
            let shifted_pp = WireId(out.0 + 40000 + j);
            self.init_wire(shifted_pp);

            // Shift partial_products[j] left by j positions
            for i in 0..self.bit_width {
                if i < j {
                    // Lower bits are 0
                    let zero_wire = format!("zero_shift_{}_{}", j, i);
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
                    let zero_wire = format!("zero_copy_{}_{}", j, i);
                    let temp = self.wire_bit_name(in1, 0);
                    self.circuit
                        .add_gate(xor_logic(), &[&temp, &temp], &zero_wire);
                    self.circuit
                        .add_gate(xor_logic(), &[&pp_bit, &zero_wire], &shifted_bit);
                }
            }

            // Add to accumulator
            let new_acc = WireId(out.0 + 50000 + j);
            self.init_wire(new_acc);
            self.build_add_internal(accumulator, shifted_pp, new_acc);
            accumulator = new_acc;
        }

        // Copy accumulator to output (only lower bits, multiplication can overflow)
        for bit_idx in 0..self.bit_width {
            let acc_bit = self.wire_bit_name(accumulator, bit_idx);
            let out_bit = self.wire_bit_name(out, bit_idx);

            // Copy using identity: a XOR 0 = a
            let zero_wire = format!("zero_final_{}", bit_idx);
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
    ) -> (Circuit, std::collections::HashMap<String, u128>, std::collections::HashMap<String, [u128; 2]>) {
        self.circuit.garble();
        let output_label_pairs = self
            .circuit
            .outputs
            .iter()
            .filter_map(|name| self.circuit.labels.get(name).map(|&p| (name.clone(), p)))
            .collect();
        (self.circuit.clone(), self.input_labels.clone(), output_label_pairs)
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

                                // this part should be replaced by sending the result to the garbler and awaiting
                                if let Some(&output_label) = results.get(&bit_wire_name) {
                                    let labels: &[u128; 2] = &self.circuit.labels[&bit_wire_name];
                                    let bit = if output_label == labels[1] {
                                        1u64
                                    } else {
                                        0u64
                                    };
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

            _ => Err(BackendError::BackendError(format!(
                "Instruction {:?} not yet implemented",
                instruction
            ))),
        }
    }
}
