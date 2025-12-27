use crate::vm::{Backend, BackendError, VMState};
use ir::lir::{Program, WireId};

pub fn execute_program<B: Backend>(
    program: &Program,
    backend: &mut B,
    inputs: &[(WireId, ir::lir::PartyId, u64)],
) -> Result<Vec<(WireId, u64)>, BackendError> {
    let num_wires = program.circuit.gates.iter()
        .map(|g| g.output.0 as usize)
        .chain(program.circuit.inputs.iter().map(|i| i.wire.0 as usize))
        .max()
        .unwrap_or(0) + 1;
    
    let mut state = VMState::new(
        num_wires,
        program.metadata.field_modulus.unwrap_or(2_u64.pow(63) - 1),
    );
    
    for (wire, _party, value) in inputs {
        let visibility = program.circuit.inputs.iter()
            .find(|i| i.wire == *wire)
            .map(|i| i.visibility)
            .unwrap_or(ir::lir::Visibility::Public);
        
        backend.set_input(*wire, *value, visibility, &mut state)?;
    }
    
    let instructions = crate::compiler::compile_to_vm_instructions(&program.circuit);
    
    for instruction in &instructions {
        backend.execute_instruction(instruction, &mut state)?;
    }
    
    let mut outputs = Vec::new();
    for output_wire in &program.circuit.outputs {
        let value = backend.get_output(*output_wire, &state)?;
        outputs.push((*output_wire, value));
    }
    
    Ok(outputs)
}