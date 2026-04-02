// Compiler from LIR to VM instructions

use crate::vm::{Instruction, VisibilityPair};
use ir::lir::{Circuit, Gate, GateType, WireId, Visibility};
use std::collections::HashMap;

struct CircuitIndex<'a> {
    /// WireId -> Visibility for all input wires.
    input_visibility: HashMap<WireId, Visibility>,
    /// WireId -> field_size for constant-output gates.
    constant_field_size: HashMap<WireId, u64>,
    circuit: &'a Circuit,
}

impl<'a> CircuitIndex<'a> {
    fn build(circuit: &'a Circuit) -> Self {
        let input_visibility: HashMap<WireId, Visibility> = circuit
            .inputs
            .iter()
            .map(|i| (i.wire, i.visibility))
            .collect();

        let constant_field_size: HashMap<WireId, u64> = circuit
            .gates
            .iter()
            .filter_map(|g| {
                if let GateType::Constant { field_size, .. } = g.gate_type {
                    Some((g.output, field_size))
                } else {
                    None
                }
            })
            .collect();

        Self { input_visibility, constant_field_size, circuit }
    }

    fn wire_visibility(&self, wire: WireId) -> Option<Visibility> {
        self.input_visibility.get(&wire).copied()
    }

    fn visibility_pair(&self, inputs: &[WireId]) -> VisibilityPair {
        let left  = self.wire_visibility(inputs[0]).unwrap_or(Visibility::Public);
        let right = self.wire_visibility(inputs[1]).unwrap_or(Visibility::Public);
        VisibilityPair::new(left, right)
    }

    fn field_size(&self, wire: WireId) -> u64 {
        self.constant_field_size.get(&wire).copied().unwrap_or(64)
    }
}

/// Group instructions by circuit level (topological depth).
///
/// Gates at the same level have no dependencies on each other and can be
/// executed in parallel.  Level 0 = circuit inputs; each gate's level is
/// `max(level of its inputs) + 1`.
pub fn levelized_instructions(circuit: &Circuit) -> Vec<Vec<Instruction>> {
    let mut wire_level: HashMap<WireId, u32> = circuit
        .inputs
        .iter()
        .map(|i| (i.wire, 0u32))
        .collect();

    let mut sorted_gates: Vec<&Gate> = circuit.gates.iter().collect();
    sorted_gates.sort_by_key(|g| g.id.0);

    let mut gate_levels: Vec<u32> = Vec::with_capacity(sorted_gates.len());
    for gate in &sorted_gates {
        let lv = gate
            .inputs
            .iter()
            .map(|w| wire_level.get(w).copied().unwrap_or(0))
            .max()
            .unwrap_or(0)
            + 1;
        wire_level.insert(gate.output, lv);
        gate_levels.push(lv);
    }

    if gate_levels.is_empty() {
        return Vec::new();
    }

    let max_level = *gate_levels.iter().max().unwrap() as usize;
    let idx = CircuitIndex::build(circuit);
    let mut levels: Vec<Vec<Instruction>> = vec![Vec::new(); max_level + 1];
    for (gate, &lv) in sorted_gates.iter().zip(&gate_levels) {
        levels[lv as usize].push(gate_to_instruction(gate, &idx));
    }
    levels
}

/// Convert LIR circuit to VM instructions
pub fn compile_to_vm_instructions(circuit: &Circuit) -> Vec<Instruction> {
    let idx = CircuitIndex::build(circuit);

    let mut gates: Vec<&Gate> = circuit.gates.iter().collect();
    gates.sort_by_key(|g| g.id.0);

    gates
        .iter()
        .map(|gate| gate_to_instruction(gate, &idx))
        .collect()
}

fn gate_to_instruction(gate: &Gate, idx: &CircuitIndex<'_>) -> Instruction {
    match &gate.gate_type {
        GateType::And => Instruction::And {
            vis:    idx.visibility_pair(&gate.inputs),
            input1: gate.inputs[0],
            input2: gate.inputs[1],
            output: gate.output,
        },
        GateType::Xor => Instruction::Xor {
            vis:    idx.visibility_pair(&gate.inputs),
            input1: gate.inputs[0],
            input2: gate.inputs[1],
            output: gate.output,
        },
        GateType::Not => Instruction::Not {
            vis:    idx.wire_visibility(gate.inputs[0]).unwrap_or(Visibility::Public),
            input:  gate.inputs[0],
            output: gate.output,
        },
        GateType::Or => Instruction::Or {
            vis:    idx.visibility_pair(&gate.inputs),
            input1: gate.inputs[0],
            input2: gate.inputs[1],
            output: gate.output,
        },
        GateType::Add => Instruction::Add {
            vis:        idx.visibility_pair(&gate.inputs),
            input1:     gate.inputs[0],
            input2:     gate.inputs[1],
            output:     gate.output,
            field_size: idx.field_size(gate.inputs[0]),
        },
        GateType::Mul => Instruction::Mul {
            vis:        idx.visibility_pair(&gate.inputs),
            input1:     gate.inputs[0],
            input2:     gate.inputs[1],
            output:     gate.output,
            field_size: idx.field_size(gate.inputs[0]),
        },
        GateType::Sub => Instruction::Sub {
            vis:        idx.visibility_pair(&gate.inputs),
            input1:     gate.inputs[0],
            input2:     gate.inputs[1],
            output:     gate.output,
            field_size: idx.field_size(gate.inputs[0]),
        },
        GateType::Div => Instruction::Div {
            vis:        idx.visibility_pair(&gate.inputs),
            input1:     gate.inputs[0],
            input2:     gate.inputs[1],
            output:     gate.output,
            field_size: idx.field_size(gate.inputs[0]),
        },
        GateType::Mod => Instruction::Mod {
            vis:        idx.visibility_pair(&gate.inputs),
            input1:     gate.inputs[0],
            input2:     gate.inputs[1],
            output:     gate.output,
            field_size: idx.field_size(gate.inputs[0]),
        },
        GateType::Constant { value, field_size } => Instruction::Constant {
            value:      *value,
            output:     gate.output,
            field_size: *field_size,
            visibility: Visibility::Public,
        },
        GateType::AddConstant { constant, field_size } => Instruction::AddConstant {
            vis:        idx.wire_visibility(gate.inputs[0]).unwrap_or(Visibility::Public),
            input:      gate.inputs[0],
            constant:   *constant,
            output:     gate.output,
            field_size: *field_size,
        },
        GateType::MulConstant { constant, field_size } => Instruction::MulConstant {
            vis:        idx.wire_visibility(gate.inputs[0]).unwrap_or(Visibility::Public),
            input:      gate.inputs[0],
            constant:   *constant,
            output:     gate.output,
            field_size: *field_size,
        },
        GateType::SubConstant { constant, field_size } => Instruction::SubConstant {
            vis:        idx.wire_visibility(gate.inputs[0]).unwrap_or(Visibility::Public),
            input:      gate.inputs[0],
            constant:   *constant,
            output:     gate.output,
            field_size: *field_size,
        },
        GateType::LessThan => Instruction::LessThan {
            vis:    idx.visibility_pair(&gate.inputs),
            input1: gate.inputs[0],
            input2: gate.inputs[1],
            output: gate.output,
        },
        GateType::Equal => Instruction::Equal {
            vis:    idx.visibility_pair(&gate.inputs),
            input1: gate.inputs[0],
            input2: gate.inputs[1],
            output: gate.output,
        },
        GateType::Select => {
            let cond_vis   = idx.wire_visibility(gate.inputs[0]).unwrap_or(Visibility::Public);
            let then_vis   = idx.wire_visibility(gate.inputs[1]).unwrap_or(Visibility::Public);
            let else_vis   = idx.wire_visibility(gate.inputs[2]).unwrap_or(Visibility::Public);
            let output_vis = if cond_vis == Visibility::Secret
                || then_vis == Visibility::Secret
                || else_vis == Visibility::Secret
            {
                Visibility::Secret
            } else {
                Visibility::Public
            };
            Instruction::Select {
                output_vis,
                condition: gate.inputs[0],
                then_val:  gate.inputs[1],
                else_val:  gate.inputs[2],
                output:    gate.output,
            }
        }
    }
}

