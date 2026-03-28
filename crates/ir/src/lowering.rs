// HIR to LIR Lowering Pass
// Converts high-level IR operations to gate-level circuit representation

use std::collections::HashMap;
use crate::hir::{self, *};
use crate::lir::{self, CircuitBuilder, Metadata, Program, Statistics, WireId, GateType};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoweringError {
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),
    
    #[error("Type mismatch: {0}")]
    TypeMismatch(String),
    
    #[error("Undefined value: {0}")]
    UndefinedValue(String),
    
    #[error("Control flow error: {0}")]
    ControlFlowError(String),
}

pub type LoweringResult<T> = Result<T, LoweringError>;

/// Lower a HIR program to LIR
pub fn lower_hir_to_lir(hir: &HirProgram, metadata: Metadata) -> LoweringResult<Program> {
    let mut lowerer = Lowerer::new();
    lowerer.lower_program(hir, metadata)
}

struct Lowerer {
    builder: CircuitBuilder,
    value_to_wire: HashMap<ValueId, WireId>,
    param_to_wire: HashMap<usize, WireId>, // Parameter index -> wire
    constant_wires: HashMap<HirConstant, WireId>,
    field_size: u64, // Current field size context
}

impl Lowerer {
    fn new() -> Self {
        Self {
            builder: lir::CircuitBuilder::new(),
            value_to_wire: HashMap::new(),
            param_to_wire: HashMap::new(),
            constant_wires: HashMap::new(),
            field_size: 64, // Default field size
        }
    }

    fn lower_program(&mut self, hir: &HirProgram, metadata: Metadata) -> LoweringResult<Program> {
        // Lower the last function — by convention, helper functions come first and
        // the circuit entry point is last. Helper functions are inlined at codegen
        // time, so only the entry function's HIR needs to be lowered to LIR.
        if let Some(function) = hir.functions.last() {
            self.lower_function(function)?;
        }

        // Build the circuit (take ownership of builder)
        let builder = std::mem::replace(&mut self.builder, lir::CircuitBuilder::new());
        let program = builder.build(metadata);
        Ok(program)
    }

    fn lower_function(&mut self, function: &HirFunction) -> LoweringResult<()> {
        // Lower parameters as inputs (party assignment left to VM/executor)
        for (idx, param) in function.params.iter().enumerate() {
            let wire = self.builder.add_input(
                self.convert_visibility(param.visibility),
                Some(param.name.clone()),
            );
            self.param_to_wire.insert(idx, wire);
        }

        // Lower blocks in topological order (simplified - assumes entry block first)
        self.lower_block(function.entry_block, &function.blocks)?;

        // Lower all other blocks
        for (block_id, _block) in &function.blocks {
            if *block_id != function.entry_block {
                self.lower_block(*block_id, &function.blocks)?;
            }
        }

        Ok(())
    }

    fn lower_block(&mut self, block_id: BlockId, blocks: &HashMap<BlockId, HirBlock>) -> LoweringResult<()> {
        let block = blocks.get(&block_id)
            .ok_or_else(|| LoweringError::ControlFlowError(format!("Block {:?} not found", block_id)))?;

        // Lower instructions
        for instruction in &block.instructions {
            self.lower_instruction(instruction)?;
        }

        // Lower terminator
        self.lower_terminator(&block.terminator, blocks)?;

        Ok(())
    }

    fn lower_instruction(&mut self, instruction: &HirInstruction) -> LoweringResult<()> {
        let output_wire = match &instruction.kind {
            HirInstructionKind::Add { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::Add, vec![left_wire, right_wire])
            }
            HirInstructionKind::Sub { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::Sub, vec![left_wire, right_wire])
            }
            HirInstructionKind::Mul { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::Mul, vec![left_wire, right_wire])
            }
            HirInstructionKind::Div { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::Div, vec![left_wire, right_wire])
            }
            HirInstructionKind::Mod { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::Mod, vec![left_wire, right_wire])
            }
            HirInstructionKind::BitwiseAnd { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::And, vec![left_wire, right_wire])
            }
            HirInstructionKind::BitwiseOr { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::Or, vec![left_wire, right_wire])
            }
            HirInstructionKind::BitwiseXor { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::Xor, vec![left_wire, right_wire])
            }
            HirInstructionKind::ShiftLeft { value, amount } => {
                // Shift left: multiply by 2^amount
                // For now, simplified - would need to handle variable shifts
                let value_wire = self.get_wire_for_value(value)?;
                let amount_wire = self.get_wire_for_value(amount)?;
                // Simplified: treat as multiplication (would need proper shift gates)
                self.builder.add_gate(GateType::Mul, vec![value_wire, amount_wire])
            }
            HirInstructionKind::ShiftRight { value, amount } => {
                // Shift right: divide by 2^amount
                let value_wire = self.get_wire_for_value(value)?;
                let amount_wire = self.get_wire_for_value(amount)?;
                // Simplified: treat as division
                self.builder.add_gate(GateType::Div, vec![value_wire, amount_wire])
            }
            HirInstructionKind::LogicalAnd { left, right } => {
                // Logical AND on booleans -> bitwise AND
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::And, vec![left_wire, right_wire])
            }
            HirInstructionKind::LogicalOr { left, right } => {
                // Logical OR: a || b = !(!a && !b)
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                
                let not_left = self.builder.add_gate(GateType::Not, vec![left_wire]);
                let not_right = self.builder.add_gate(GateType::Not, vec![right_wire]);
                let and_result = self.builder.add_gate(GateType::And, vec![not_left, not_right]);
                self.builder.add_gate(GateType::Not, vec![and_result])
            }
            HirInstructionKind::Equal { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::Equal, vec![left_wire, right_wire])
            }
            HirInstructionKind::NotEqual { left, right } => {
                // a != b  ≡  NOT(a == b)
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                let eq = self.builder.add_gate(GateType::Equal, vec![left_wire, right_wire]);
                self.builder.add_gate(GateType::Not, vec![eq])
            }
            HirInstructionKind::LessThan { left, right } => {
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::LessThan, vec![left_wire, right_wire])
            }
            HirInstructionKind::LessThanOrEqual { left, right } => {
                // a <= b  ≡  NOT(b < a)
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                let gt = self.builder.add_gate(GateType::LessThan, vec![right_wire, left_wire]);
                self.builder.add_gate(GateType::Not, vec![gt])
            }
            HirInstructionKind::GreaterThan { left, right } => {
                // a > b  ≡  b < a  (swap operands)
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                self.builder.add_gate(GateType::LessThan, vec![right_wire, left_wire])
            }
            HirInstructionKind::GreaterThanOrEqual { left, right } => {
                // a >= b  ≡  NOT(a < b)
                let left_wire = self.get_wire_for_value(left)?;
                let right_wire = self.get_wire_for_value(right)?;
                let lt = self.builder.add_gate(GateType::LessThan, vec![left_wire, right_wire]);
                self.builder.add_gate(GateType::Not, vec![lt])
            }
            HirInstructionKind::Negate { value } => {
                // Negate: multiply by -1 (or subtract from 0)
                let value_wire = self.get_wire_for_value(value)?;
                let zero = self.get_constant_wire(HirConstant::Field { value: 0, size: self.field_size })?;
                self.builder.add_gate(GateType::Sub, vec![zero, value_wire])
            }
            HirInstructionKind::Not { value } => {
                let value_wire = self.get_wire_for_value(value)?;
                self.builder.add_gate(GateType::Not, vec![value_wire])
            }
            HirInstructionKind::BitwiseNot { value } => {
                // Bitwise NOT: XOR with all 1s
                // Simplified: for now, use NOT gate
                let value_wire = self.get_wire_for_value(value)?;
                self.builder.add_gate(GateType::Not, vec![value_wire])
            }
            HirInstructionKind::ArrayLoad { array: _, index: _ } => {
                // Array access would need memory gates
                // For now, simplified
                return Err(LoweringError::UnsupportedOperation("ArrayLoad not yet implemented".to_string()));
            }
            HirInstructionKind::ArrayStore { .. } => {
                return Err(LoweringError::UnsupportedOperation("ArrayStore not yet implemented".to_string()));
            }
            HirInstructionKind::ArrayAlloc { .. } => {
                return Err(LoweringError::UnsupportedOperation("ArrayAlloc not yet implemented".to_string()));
            }
            HirInstructionKind::StructField { .. } => {
                return Err(LoweringError::UnsupportedOperation("StructField not yet implemented".to_string()));
            }
            HirInstructionKind::StructAlloc { .. } => {
                return Err(LoweringError::UnsupportedOperation("StructAlloc not yet implemented".to_string()));
            }
            HirInstructionKind::Call { .. } => {
                return Err(LoweringError::UnsupportedOperation("Function calls not yet implemented".to_string()));
            }
            HirInstructionKind::Phi { .. } => {
                return Err(LoweringError::UnsupportedOperation("Phi nodes not yet implemented".to_string()));
            }
            HirInstructionKind::Select { condition, then_val, else_val } => {
                let cond_wire = self.get_wire_for_value(condition)?;
                let then_wire = self.get_wire_for_value(then_val)?;
                let else_wire = self.get_wire_for_value(else_val)?;
                self.builder.add_gate(GateType::Select, vec![cond_wire, then_wire, else_wire])
            }
        };

        // Map instruction output to wire
        self.value_to_wire.insert(instruction.id, output_wire);
        Ok(())
    }

    fn lower_terminator(&mut self, terminator: &HirTerminator, _blocks: &HashMap<BlockId, HirBlock>) -> LoweringResult<()> {
        match terminator {
            HirTerminator::Return { value } => {
                let wire = self.get_wire_for_value(value)?;
                self.builder.add_output(wire);
            }
            HirTerminator::Branch { condition, then_block: _, else_block: _ } => {
                // Conditional branches would need multiplexer gates
                // For now, we'll just evaluate the condition
                self.get_wire_for_value(condition)?;
            }
            HirTerminator::Jump { .. } => {
                // Jumps don't generate gates
            }
            HirTerminator::Loop { .. } => {
                return Err(LoweringError::UnsupportedOperation("Loops not yet implemented".to_string()));
            }
            HirTerminator::Unreachable => {
                // Unreachable blocks don't generate gates
            }
        }
        Ok(())
    }

    fn get_wire_for_value(&mut self, value: &HirValue) -> LoweringResult<WireId> {
        match value {
            HirValue::Instruction(id) => {
                self.value_to_wire.get(id)
                    .copied()
                    .ok_or_else(|| LoweringError::UndefinedValue(format!("Instruction {:?} not found", id)))
            }
            HirValue::Param(idx) => {
                self.param_to_wire.get(idx)
                    .copied()
                    .ok_or_else(|| LoweringError::UndefinedValue(format!("Parameter {} not found", idx)))
            }
            HirValue::Constant(constant) => {
                self.get_constant_wire(*constant)
            }
        }
    }

    fn get_constant_wire(&mut self, constant: HirConstant) -> LoweringResult<WireId> {
        if let Some(wire) = self.constant_wires.get(&constant) {
            return Ok(*wire);
        }

        let wire = match constant {
            HirConstant::Field { value, size } => {
                self.field_size = size;
                self.builder.add_constant(value, size)
            }
            HirConstant::Bool(true) => {
                self.builder.add_constant(1, 1) // Boolean true as 1-bit value
            }
            HirConstant::Bool(false) => {
                self.builder.add_constant(0, 1) // Boolean false as 0-bit value
            }
        };

        self.constant_wires.insert(constant, wire);
        Ok(wire)
    }

    fn convert_visibility(&self, vis: hir::Visibility) -> lir::Visibility {
        match vis {
            hir::Visibility::Public => lir::Visibility::Public,
            hir::Visibility::Secret => lir::Visibility::Secret,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::*;
    use crate::lir::*;

    fn create_simple_hir_function() -> HirFunction {
        let mut builder = HirBuilder::new();
        let entry_block = builder.create_block();
        builder.set_current_block(entry_block);

        // Create a simple add function: return a + b
        let param_a = HirValue::Param(0);
        let param_b = HirValue::Param(1);
        
        let add_id = builder.add_instruction(
            HirInstructionKind::Add {
                left: param_a,
                right: param_b,
            },
            HirType::Field { size: 64 },
        );

        builder.set_terminator(HirTerminator::Return {
            value: HirValue::Instruction(add_id),
        });

        let blocks = builder.get_blocks();
        
        HirFunction {
            name: "add".to_string(),
            params: vec![
                HirParam {
                    name: "a".to_string(),
                    ty: HirType::Field { size: 64 },
                    visibility: hir::Visibility::Public,
                },
                HirParam {
                    name: "b".to_string(),
                    ty: HirType::Field { size: 64 },
                    visibility: hir::Visibility::Public,
                },
            ],
            return_type: HirType::Field { size: 64 },
            entry_block,
            blocks,
        }
    }

    #[test]
    fn test_lower_simple_function() {
        let function = create_simple_hir_function();
        let program = HirProgram {
            structs: Vec::new(),
            functions: vec![function],
        };

        let metadata = Metadata {
            version: "1.0".to_string(),
            source_file: "test.mpc".to_string(),
            function_name: "add".to_string(),
            field_modulus: Some(2_u64.pow(63) - 1), // Use a safe value that doesn't overflow
            statistics: Statistics {
                total_gates: 0,
                gate_counts: HashMap::new(),
                circuit_depth: 0,
                num_inputs: 0,
                num_outputs: 0,
                num_wires: 0,
            },
        };

        let result = lower_hir_to_lir(&program, metadata);
        assert!(result.is_ok());
        
        let lir = result.unwrap();
        assert_eq!(lir.circuit.inputs.len(), 2); // Two parameters
        assert!(!lir.circuit.gates.is_empty()); // Should have at least an Add gate
        assert_eq!(lir.circuit.outputs.len(), 1); // One return value
    }

    #[test]
    fn test_lower_constant() {
        let mut builder = HirBuilder::new();
        let entry_block = builder.create_block();
        builder.set_current_block(entry_block);

        let constant = HirValue::Constant(HirConstant::Field { value: 42, size: 64 });
        let return_id = builder.add_instruction(
            HirInstructionKind::Add {
                left: constant.clone(),
                right: HirValue::Constant(HirConstant::Field { value: 0, size: 64 }),
            },
            HirType::Field { size: 64 },
        );

        builder.set_terminator(HirTerminator::Return {
            value: HirValue::Instruction(return_id),
        });

        let blocks = builder.get_blocks();
        
        let function = HirFunction {
            name: "constant".to_string(),
            params: Vec::new(),
            return_type: HirType::Field { size: 64 },
            entry_block,
            blocks,
        };

        let program = HirProgram {
            structs: Vec::new(),
            functions: vec![function],
        };

        let metadata = Metadata {
            version: "1.0".to_string(),
            source_file: "test.mpc".to_string(),
            function_name: "constant".to_string(),
            field_modulus: Some(2_u64.pow(63) - 1), // Use a safe value that doesn't overflow
            statistics: Statistics {
                total_gates: 0,
                gate_counts: HashMap::new(),
                circuit_depth: 0,
                num_inputs: 0,
                num_outputs: 0,
                num_wires: 0,
            },
        };

        let result = lower_hir_to_lir(&program, metadata);
        assert!(result.is_ok());
    }
}

