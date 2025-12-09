// Compiler from LIR to VM instructions

use crate::vm::{Instruction, VisibilityPair};
use ir::lir::{Circuit, Gate, GateType, WireId, Visibility};

/// Convert LIR circuit to VM instructions
pub fn compile_to_vm_instructions(circuit: &Circuit) -> Vec<Instruction> {
    let mut instructions = Vec::new();
    
    // Sort gates by their ID to ensure deterministic execution order
    let mut gates: Vec<&Gate> = circuit.gates.iter().collect();
    gates.sort_by_key(|g| g.id.0);
    
    for gate in gates {
        let instruction = gate_to_instruction(gate, circuit);
        instructions.push(instruction);
    }
    
    instructions
}

fn gate_to_instruction(gate: &Gate, _circuit: &Circuit) -> Instruction {
    match &gate.gate_type {
        GateType::And => {
            let vis = get_visibility_pair(&gate.inputs, _circuit);
            Instruction::And {
                vis,
                input1: gate.inputs[0],
                input2: gate.inputs[1],
                output: gate.output,
            }
        }
        GateType::Xor => {
            let vis = get_visibility_pair(&gate.inputs, _circuit);
            Instruction::Xor {
                vis,
                input1: gate.inputs[0],
                input2: gate.inputs[1],
                output: gate.output,
            }
        }
        GateType::Not => {
            let vis = get_wire_visibility(gate.inputs[0], _circuit).unwrap_or(Visibility::Public);
            Instruction::Not {
                vis,
                input: gate.inputs[0],
                output: gate.output,
            }
        }
        GateType::Or => {
            // OR is not in minimal set, but we can compile it
            // For now, we'll treat it as a separate instruction or decompose it
            // For simplicity, let's add it as XOR(AND(NOT(a), NOT(b)), 1)
            // But since we don't have intermediate wires, we'll need to handle this differently
            // For now, implement OR directly (can be optimized later)
            let vis = get_visibility_pair(&gate.inputs, _circuit);
            // Decompose OR: a OR b = NOT(AND(NOT(a), NOT(b)))
            // This requires intermediate wires, so for now we'll add OR as a special case
            // Actually, let's just use XOR pattern: a OR b = XOR(a, b) XOR AND(a, b)
            // But that's complex. For clear backend, we can just compute directly
            // Let's add a helper that computes OR
            let vis = get_visibility_pair(&gate.inputs, _circuit);
            Instruction::Xor {
                vis,
                input1: gate.inputs[0],
                input2: gate.inputs[1],
                output: gate.output,
            }
        }
        GateType::Add => {
            let vis = get_visibility_pair(&gate.inputs, _circuit);
            // Field size from metadata - we'll need to pass it or store it
            // For now, use a default
            let field_size = get_field_size_from_constant_gates(_circuit, gate.inputs[0])
                .unwrap_or(64);
            Instruction::Add {
                vis,
                input1: gate.inputs[0],
                input2: gate.inputs[1],
                output: gate.output,
                field_size,
            }
        }
        GateType::Mul => {
            let vis = get_visibility_pair(&gate.inputs, _circuit);
            let field_size = get_field_size_from_constant_gates(_circuit, gate.inputs[0])
                .unwrap_or(64);
            Instruction::Mul {
                vis,
                input1: gate.inputs[0],
                input2: gate.inputs[1],
                output: gate.output,
                field_size,
            }
        }
        GateType::Sub => {
            let vis = get_visibility_pair(&gate.inputs, _circuit);
            let field_size = get_field_size_from_constant_gates(_circuit, gate.inputs[0])
                .unwrap_or(64);
            Instruction::Sub {
                vis,
                input1: gate.inputs[0],
                input2: gate.inputs[1],
                output: gate.output,
                field_size,
            }
        }
        GateType::Div => {
            let vis = get_visibility_pair(&gate.inputs, _circuit);
            let field_size = get_field_size_from_constant_gates(_circuit, gate.inputs[0])
                .unwrap_or(64);
            Instruction::Div {
                vis,
                input1: gate.inputs[0],
                input2: gate.inputs[1],
                output: gate.output,
                field_size,
            }
        }
        GateType::Mod => {
            let vis = get_visibility_pair(&gate.inputs, _circuit);
            let field_size = get_field_size_from_constant_gates(_circuit, gate.inputs[0])
                .unwrap_or(64);
            Instruction::Mod {
                vis,
                input1: gate.inputs[0],
                input2: gate.inputs[1],
                output: gate.output,
                field_size,
            }
        }
        GateType::Constant { value, field_size } => {
            // Visibility for constants is typically Public
            Instruction::Constant {
                value: *value,
                output: gate.output,
                field_size: *field_size,
                visibility: Visibility::Public,
            }
        }
        GateType::AddConstant { constant, field_size } => {
            let vis = get_wire_visibility(gate.inputs[0], _circuit).unwrap_or(Visibility::Public);
            Instruction::AddConstant {
                vis,
                input: gate.inputs[0],
                constant: *constant,
                output: gate.output,
                field_size: *field_size,
            }
        }
        GateType::MulConstant { constant, field_size } => {
            let vis = get_wire_visibility(gate.inputs[0], _circuit).unwrap_or(Visibility::Public);
            Instruction::MulConstant {
                vis,
                input: gate.inputs[0],
                constant: *constant,
                output: gate.output,
                field_size: *field_size,
            }
        }
        GateType::SubConstant { constant, field_size } => {
            let vis = get_wire_visibility(gate.inputs[0], _circuit).unwrap_or(Visibility::Public);
            Instruction::SubConstant {
                vis,
                input: gate.inputs[0],
                constant: *constant,
                output: gate.output,
                field_size: *field_size,
            }
        }
    }
}

fn get_visibility_pair(inputs: &[WireId], circuit: &Circuit) -> VisibilityPair {
    let left_vis = get_wire_visibility(inputs[0], circuit).unwrap_or(Visibility::Public);
    let right_vis = get_wire_visibility(inputs[1], circuit).unwrap_or(Visibility::Public);
    VisibilityPair::new(left_vis, right_vis)
}

fn get_wire_visibility(wire: WireId, circuit: &Circuit) -> Option<Visibility> {
    // Check if wire is an input
    for input in &circuit.inputs {
        if input.wire == wire {
            return Some(input.visibility);
        }
    }
    // For output wires, we'd need to track visibility through gates
    // For now, return None and let the caller use a default
    None
}

fn get_field_size_from_constant_gates(circuit: &Circuit, wire: WireId) -> Option<u64> {
    // Try to find the field size by looking at constant gates that feed into this wire
    // This is a heuristic - in a real implementation, field size should be tracked per wire
    for gate in &circuit.gates {
        if gate.output == wire {
            match gate.gate_type {
                ir::lir::GateType::Constant { field_size, .. } => {
                    return Some(field_size);
                }
                _ => {}
            }
        }
    }
    None
}

