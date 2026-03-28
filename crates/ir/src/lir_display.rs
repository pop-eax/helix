// Display utilities for LIR

use crate::lir::*;

pub fn display_lir_program(program: &Program) -> String {
    let mut result = String::new();
    
    // Display metadata
    result.push_str("=== Metadata ===\n");
    result.push_str(&format!("Version: {}\n", program.metadata.version));
    result.push_str(&format!("Source: {}\n", program.metadata.source_file));
    result.push_str(&format!("Function: {}\n", program.metadata.function_name));
    if let Some(modulus) = program.metadata.field_modulus {
        result.push_str(&format!("Field Modulus: {}\n", modulus));
    }
    result.push_str("\n");
    
    // Display statistics
    result.push_str("=== Statistics ===\n");
    result.push_str(&format!("Total Gates: {}\n", program.metadata.statistics.total_gates));
    result.push_str(&format!("Circuit Depth: {}\n", program.metadata.statistics.circuit_depth));
    result.push_str(&format!("Inputs: {}\n", program.metadata.statistics.num_inputs));
    result.push_str(&format!("Outputs: {}\n", program.metadata.statistics.num_outputs));
    result.push_str(&format!("Wires: {}\n", program.metadata.statistics.num_wires));
    result.push_str("\n");
    
    // Display inputs
    result.push_str("=== Inputs ===\n");
    for input in &program.circuit.inputs {
        let party_str = input.party.map(|p| p.0.to_string()).unwrap_or_else(|| "None".to_string());
        result.push_str(&format!(
            "  Wire {}: Party {}, Visibility: {:?}, Name: {:?}\n",
            input.wire.0,
            party_str,
            input.visibility,
            input.name
        ));
    }
    result.push_str("\n");
    
    // Display gates
    result.push_str("=== Gates ===\n");
    for gate in &program.circuit.gates {
        result.push_str(&format!(
            "  Gate {}: {} -> Wire {}\n",
            gate.id.0,
            display_gate_type(&gate.gate_type, &gate.inputs),
            gate.output.0
        ));
    }
    result.push_str("\n");
    
    // Display outputs
    result.push_str("=== Outputs ===\n");
    for output in &program.circuit.outputs {
        result.push_str(&format!("  Wire {}\n", output.0));
    }
    
    result
}

fn display_gate_type(gate_type: &GateType, inputs: &[WireId]) -> String {
    let inputs_str: Vec<String> = inputs.iter().map(|w| format!("w{}", w.0)).collect();
    let inputs_list = inputs_str.join(", ");
    
    match gate_type {
        GateType::And => format!("AND({})", inputs_list),
        GateType::Xor => format!("XOR({})", inputs_list),
        GateType::Not => format!("NOT({})", inputs_list),
        GateType::Or => format!("OR({})", inputs_list),
        GateType::Add => format!("ADD({})", inputs_list),
        GateType::Mul => format!("MUL({})", inputs_list),
        GateType::Sub => format!("SUB({})", inputs_list),
        GateType::Div => format!("DIV({})", inputs_list),
        GateType::Mod => format!("MOD({})", inputs_list),
        GateType::Constant { value, field_size } => {
            format!("CONST({}, size={})", value, field_size)
        }
        GateType::AddConstant { constant, field_size } => {
            format!("ADD_CONST({}, size={})", constant, field_size)
        }
        GateType::MulConstant { constant, field_size } => {
            format!("MUL_CONST({}, size={})", constant, field_size)
        }
        GateType::SubConstant { constant, field_size } => {
            format!("SUB_CONST({}, size={})", constant, field_size)
        }
        GateType::LessThan => format!("LT({})", inputs_list),
        GateType::Equal => format!("EQ({})", inputs_list),
        GateType::Select => format!("SELECT({})", inputs_list),
    }
}

