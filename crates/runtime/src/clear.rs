use crate::vm::{Backend, BackendError, Instruction, VMState, WireValue};
use ir::lir::{WireId, Visibility};
use rayon::prelude::*;

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

/// Read-only gate evaluation — returns `(output_wire, value, visibility)` without
/// mutating state.  Used by the parallel batch executor.
fn eval_clear_gate(
    instr: &Instruction,
    state: &VMState,
    field_modulus: Option<u64>,
) -> Result<(WireId, u64, Visibility), BackendError> {
    let reduce = |v: u64, size: u64| -> u64 {
        if let Some(m) = field_modulus {
            v % m
        } else {
            let max = 1u64 << size.min(63);
            v % max
        }
    };
    let get = |wire: WireId| -> Result<u64, BackendError> {
        match state.get_wire(wire) {
            Some(WireValue::Clear(v)) => Ok(*v),
            Some(WireValue::Secret) => Err(BackendError::BackendError(format!(
                "Cannot get clear value from secret wire {wire:?}"
            ))),
            None => Err(BackendError::WireNotSet(wire)),
        }
    };

    match instr {
        Instruction::And { vis, input1, input2, output } => {
            let result = ((get(*input1)? != 0) && (get(*input2)? != 0)) as u64;
            Ok((*output, result, vis.output_visibility()))
        }
        Instruction::Xor { vis, input1, input2, output } => {
            let result = ((get(*input1)? != 0) != (get(*input2)? != 0)) as u64;
            Ok((*output, result, vis.output_visibility()))
        }
        Instruction::Not { vis, input, output } => {
            Ok((*output, (get(*input)? == 0) as u64, *vis))
        }
        Instruction::Or { vis, input1, input2, output } => {
            let result = ((get(*input1)? != 0) || (get(*input2)? != 0)) as u64;
            Ok((*output, result, vis.output_visibility()))
        }
        Instruction::Add { vis, input1, input2, output, field_size } => {
            let result = reduce(get(*input1)?.wrapping_add(get(*input2)?), *field_size);
            Ok((*output, result, vis.output_visibility()))
        }
        Instruction::Mul { vis, input1, input2, output, field_size } => {
            let result = reduce(get(*input1)?.wrapping_mul(get(*input2)?), *field_size);
            Ok((*output, result, vis.output_visibility()))
        }
        Instruction::Sub { vis, input1, input2, output, field_size } => {
            let result = reduce(get(*input1)?.wrapping_sub(get(*input2)?), *field_size);
            Ok((*output, result, vis.output_visibility()))
        }
        Instruction::Div { vis, input1, input2, output, field_size } => {
            let v2 = get(*input2)?;
            if v2 == 0 { return Err(BackendError::DivisionByZero); }
            Ok((*output, reduce(get(*input1)? / v2, *field_size), vis.output_visibility()))
        }
        Instruction::Mod { vis, input1, input2, output, field_size } => {
            let v2 = get(*input2)?;
            if v2 == 0 { return Err(BackendError::DivisionByZero); }
            Ok((*output, reduce(get(*input1)? % v2, *field_size), vis.output_visibility()))
        }
        Instruction::LessThan { vis, input1, input2, output } => {
            Ok((*output, (get(*input1)? < get(*input2)?) as u64, vis.output_visibility()))
        }
        Instruction::Equal { vis, input1, input2, output } => {
            Ok((*output, (get(*input1)? == get(*input2)?) as u64, vis.output_visibility()))
        }
        Instruction::Constant { value, output, field_size, visibility } => {
            Ok((*output, reduce(*value, *field_size), *visibility))
        }
        Instruction::AddConstant { vis, input, constant, output, field_size } => {
            Ok((*output, reduce(get(*input)?.wrapping_add(*constant), *field_size), *vis))
        }
        Instruction::MulConstant { vis, input, constant, output, field_size } => {
            Ok((*output, reduce(get(*input)?.wrapping_mul(*constant), *field_size), *vis))
        }
        Instruction::SubConstant { vis, input, constant, output, field_size } => {
            Ok((*output, reduce(get(*input)?.wrapping_sub(*constant), *field_size), *vis))
        }
        Instruction::Select { output_vis, condition, then_val, else_val, output } => {
            let result = if get(*condition)? != 0 { get(*then_val)? } else { get(*else_val)? };
            Ok((*output, result, *output_vis))
        }
    }
}

impl Backend for ClearBackend {
    fn name(&self) -> &'static str {
        "Clear"
    }
    
    fn set_input(
        &mut self,
        wire: WireId,
        value: u64,
        visibility: Visibility,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        // Clear backend just sets the wire value directly
        state.set_wire(wire, WireValue::Clear(value), visibility);
        Ok(())
    }
    
    fn get_output(
        &mut self,
        wire: WireId,
        state: &VMState,
    ) -> Result<u64, BackendError> {
        // Clear backend reads directly from state
        self.get_clear_value(state, wire)
    }

    fn execute_batch(&mut self, instructions: &[Instruction], state: &mut VMState) -> Result<(), BackendError> {
        let modulus = self.field_modulus;
        // Parallel read phase: shared borrow of state, one result per gate.
        let results: Vec<(WireId, u64, Visibility)> = {
            let s: &VMState = state;
            instructions
                .par_iter()
                .map(|instr| eval_clear_gate(instr, s, modulus))
                .collect::<Result<Vec<_>, _>>()?
        };
        // Sequential write phase: no conflicts (each gate writes its own output wire).
        for (wire, value, vis) in results {
            state.set_wire(wire, WireValue::Clear(value), vis);
        }
        Ok(())
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

            Instruction::Or { vis, input1, input2, output } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                let result = ((v1 != 0) || (v2 != 0)) as u64;
                state.set_wire(*output, WireValue::Clear(result), vis.output_visibility());
                Ok(())
            }

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
            
            Instruction::LessThan { vis, input1, input2, output } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                state.set_wire(*output, WireValue::Clear((v1 < v2) as u64), vis.output_visibility());
                Ok(())
            }

            Instruction::Equal { vis, input1, input2, output } => {
                let v1 = self.get_clear_value(state, *input1)?;
                let v2 = self.get_clear_value(state, *input2)?;
                state.set_wire(*output, WireValue::Clear((v1 == v2) as u64), vis.output_visibility());
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

            Instruction::Select { output_vis, condition, then_val, else_val, output } => {
                let cond = self.get_clear_value(state, *condition)?;
                let result = if cond != 0 {
                    self.get_clear_value(state, *then_val)?
                } else {
                    self.get_clear_value(state, *else_val)?
                };
                state.set_wire(*output, WireValue::Clear(result), *output_vis);
                Ok(())
            }
        }
    }
}

