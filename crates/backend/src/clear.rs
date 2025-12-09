// Clear (non-crypto) backend implementation
// This backend performs plain evaluation without any cryptographic operations

use crate::vm::{Backend, BackendError, Instruction, VMState, WireValue};
use ir::lir::WireId;

/// Clear backend - plain evaluation without cryptography
pub struct ClearBackend {
    field_modulus: Option<u64>,
}

impl ClearBackend {
    pub fn new(field_modulus: Option<u64>) -> Self {
        Self { field_modulus }
    }
    
    /// Perform modular arithmetic reduction
    fn reduce(&self, value: u64, field_size: u64) -> u64 {
        if let Some(modulus) = self.field_modulus {
            value % modulus
        } else {
            // If no modulus specified, use 2^field_size
            let max = 1u64 << field_size.min(63); // Avoid overflow
            value % max
        }
    }
    
    /// Get clear value from wire
    fn get_clear_value(&self, state: &VMState, wire: WireId) -> Result<u64, BackendError> {
        match state.get_wire(wire) {
            Some(WireValue::Clear(value)) => Ok(*value),
            Some(WireValue::Secret) => Err(BackendError::BackendError(
                format!("Cannot get clear value from secret wire {:?}", wire)
            )),
            None => Err(BackendError::WireNotSet(wire)),
        }
    }
}

impl Backend for ClearBackend {
    fn name(&self) -> &'static str {
        "Clear"
    }
    
    fn execute_instruction(&mut self, instruction: &Instruction, state: &mut VMState) -> Result<(), BackendError> {
        match instruction {
            // Boolean gates
            Instruction::And { vis, input1, input2, output } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                let result = (v1 != 0) && (v2 != 0);
                let output_vis = vis.output_visibility();
                state.set_wire(*output, WireValue::Clear(result as u64), output_vis);
                Ok(())
            }
            
            Instruction::Xor { vis, input1, input2, output } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                let result = (v1 != 0) != (v2 != 0);
                let output_vis = vis.output_visibility();
                state.set_wire(*output, WireValue::Clear(result as u64), output_vis);
                Ok(())
            }
            
            Instruction::Not { vis, input, output } => {
                let v = self.get_clear_value(state, *input)?;
                let result = (v == 0) as u64;
                state.set_wire(*output, WireValue::Clear(result), *vis);
                Ok(())
            }
            
            // Note: OR gate is not in the minimal instruction set
            // It should be decomposed, but for clear backend we can handle it if needed
            // For now, we don't have an OR instruction in the VM
            
            // Arithmetic gates
            Instruction::Add { vis, input1, input2, output, field_size } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                let result = self.reduce(v1.wrapping_add(v2), *field_size);
                let output_vis = vis.output_visibility();
                state.set_wire(*output, WireValue::Clear(result), output_vis);
                Ok(())
            }
            
            Instruction::Mul { vis, input1, input2, output, field_size } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                let result = self.reduce(v1.wrapping_mul(v2), *field_size);
                let output_vis = vis.output_visibility();
                state.set_wire(*output, WireValue::Clear(result), output_vis);
                Ok(())
            }
            
            Instruction::Sub { vis, input1, input2, output, field_size } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                let result = self.reduce(v1.wrapping_sub(v2), *field_size);
                let output_vis = vis.output_visibility();
                state.set_wire(*output, WireValue::Clear(result), output_vis);
                Ok(())
            }
            
            Instruction::Div { vis, input1, input2, output, field_size } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                if v2 == 0 {
                    return Err(BackendError::DivisionByZero);
                }
                let result = self.reduce(v1 / v2, *field_size);
                let output_vis = vis.output_visibility();
                state.set_wire(*output, WireValue::Clear(result), output_vis);
                Ok(())
            }
            
            Instruction::Mod { vis, input1, input2, output, field_size } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                if v2 == 0 {
                    return Err(BackendError::DivisionByZero);
                }
                let result = self.reduce(v1 % v2, *field_size);
                let output_vis = vis.output_visibility();
                state.set_wire(*output, WireValue::Clear(result), output_vis);
                Ok(())
            }
            
            // Constant operations
            Instruction::Constant { value, output, field_size, visibility } => {
                let result = self.reduce(*value, *field_size);
                state.set_wire(*output, WireValue::Clear(result), *visibility);
                Ok(())
            }
            
            Instruction::AddConstant { vis, input, constant, output, field_size } => {
                let v = self.get_clear_value(state, *input)?;
                let result = self.reduce(v.wrapping_add(*constant), *field_size);
                state.set_wire(*output, WireValue::Clear(result), *vis);
                Ok(())
            }
            
            Instruction::MulConstant { vis, input, constant, output, field_size } => {
                let v = self.get_clear_value(state, *input)?;
                let result = self.reduce(v.wrapping_mul(*constant), *field_size);
                state.set_wire(*output, WireValue::Clear(result), *vis);
                Ok(())
            }
            
            Instruction::SubConstant { vis, input, constant, output, field_size } => {
                let v = self.get_clear_value(state, *input)?;
                let result = self.reduce(v.wrapping_sub(*constant), *field_size);
                state.set_wire(*output, WireValue::Clear(result), *vis);
                Ok(())
            }
        }
    }
}

