// VM Executor - executes VM instructions using a backend

use crate::vm::{Backend, BackendError, Instruction, VMState};
use ir::lir::{Circuit, Input, Program, WireId};

/// Execute a program using the given backend
/// 
/// # Arguments
/// * `program` - The LIR program to execute
/// * `backend` - The backend implementation to use
/// * `inputs` - Input values as (wire_id, party_id, value) tuples
///              Party ID is assigned here by the VM/executor
pub fn execute_program<B: Backend>(
    program: &Program,
    backend: &mut B,
    inputs: &[(WireId, ir::lir::PartyId, u64)],
) -> Result<Vec<(WireId, u64)>, BackendError> {
    // Create VM state
    let num_wires = program.circuit.gates.iter()
        .map(|g| g.output.0 as usize)
        .chain(program.circuit.inputs.iter().map(|i| i.wire.0 as usize))
        .max()
        .unwrap_or(0) + 1;
    
    let mut state = VMState::new(
        num_wires,
        program.metadata.field_modulus.unwrap_or(2_u64.pow(63) - 1),
    );
    
    // Set input values
    // Party information is provided in the inputs tuple and can be used by backends
    // that need to know which party owns which input (e.g., for secret sharing)
    for (wire, _party, value) in inputs {
        // Find the visibility for this input wire
        let visibility = program.circuit.inputs.iter()
            .find(|i| i.wire == *wire)
            .map(|i| i.visibility)
            .unwrap_or(ir::lir::Visibility::Public);
        
        state.set_wire(*wire, crate::vm::WireValue::Clear(*value), visibility);
    }
    
    // Compile to VM instructions
    let instructions = crate::compiler::compile_to_vm_instructions(&program.circuit);
    
    // Execute instructions
    for instruction in &instructions {
        backend.execute_instruction(instruction, &mut state)?;
    }
    
    // Collect output values
    let mut outputs = Vec::new();
    for output_wire in &program.circuit.outputs {
        if let Some(crate::vm::WireValue::Clear(value)) = state.get_wire(*output_wire) {
            outputs.push((*output_wire, *value));
        } else {
            return Err(BackendError::WireNotSet(*output_wire));
        }
    }
    
    Ok(outputs)
}

