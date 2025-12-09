// Virtual Machine for MPC Backend
// Defines the minimal instruction set and VM interface

use ir::lir::{WireId, Visibility};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Visibility pair for binary operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityPair {
    pub left: Visibility,
    pub right: Visibility,
}

impl VisibilityPair {
    pub fn new(left: Visibility, right: Visibility) -> Self {
        Self { left, right }
    }
    
    /// Get the output visibility (secret if either input is secret)
    pub fn output_visibility(&self) -> Visibility {
        match (self.left, self.right) {
            (Visibility::Secret, _) | (_, Visibility::Secret) => Visibility::Secret,
            _ => Visibility::Public,
        }
    }
}

/// Minimal set of VM instructions
/// These are the primitive operations that backends must implement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Instruction {
    // Boolean gates (minimal set: AND, XOR, NOT)
    // OR can be built from AND and NOT: a OR b = NOT(AND(NOT(a), NOT(b)))
    And {
        vis: VisibilityPair,
        input1: WireId,
        input2: WireId,
        output: WireId,
    },
    Xor {
        vis: VisibilityPair,
        input1: WireId,
        input2: WireId,
        output: WireId,
    },
    Not {
        vis: Visibility,
        input: WireId,
        output: WireId,
    },
    
    // Arithmetic gates (minimal set: ADD, MUL, SUB)
    // DIV and MOD are included as they're commonly needed
    Add {
        vis: VisibilityPair,
        input1: WireId,
        input2: WireId,
        output: WireId,
        field_size: u64,
    },
    Mul {
        vis: VisibilityPair,
        input1: WireId,
        input2: WireId,
        output: WireId,
        field_size: u64,
    },
    Sub {
        vis: VisibilityPair,
        input1: WireId,
        input2: WireId,
        output: WireId,
        field_size: u64,
    },
    Div {
        vis: VisibilityPair,
        input1: WireId,
        input2: WireId,
        output: WireId,
        field_size: u64,
    },
    Mod {
        vis: VisibilityPair,
        input1: WireId,
        input2: WireId,
        output: WireId,
        field_size: u64,
    },
    
    // Constant operations
    Constant {
        value: u64,
        output: WireId,
        field_size: u64,
        visibility: Visibility,
    },
    AddConstant {
        vis: Visibility,
        input: WireId,
        constant: u64,
        output: WireId,
        field_size: u64,
    },
    MulConstant {
        vis: Visibility,
        input: WireId,
        constant: u64,
        output: WireId,
        field_size: u64,
    },
    SubConstant {
        vis: Visibility,
        input: WireId,
        constant: u64,
        output: WireId,
        field_size: u64,
    },
}

/// Wire value in the VM
/// For the clear backend, this is just the plain value
/// For crypto backends, this would be shares/ciphertexts
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WireValue {
    /// Plain value (for clear backend)
    Clear(u64),
    /// Placeholder for future crypto backends
    Secret,
}

/// VM state - tracks wire values and metadata
#[derive(Debug, Clone)]
pub struct VMState {
    /// Wire values indexed by WireId
    pub wires: Vec<Option<WireValue>>,
    /// Wire visibility
    pub wire_visibility: Vec<Visibility>,
    /// Field size for arithmetic operations
    pub field_size: u64,
}

impl VMState {
    pub fn new(num_wires: usize, field_size: u64) -> Self {
        Self {
            wires: vec![None; num_wires],
            wire_visibility: vec![Visibility::Public; num_wires],
            field_size,
        }
    }
    
    pub fn set_wire(&mut self, wire: WireId, value: WireValue, visibility: Visibility) {
        let idx = wire.0 as usize;
        if idx < self.wires.len() {
            self.wires[idx] = Some(value);
            self.wire_visibility[idx] = visibility;
        }
    }
    
    pub fn get_wire(&self, wire: WireId) -> Option<&WireValue> {
        let idx = wire.0 as usize;
        self.wires.get(idx).and_then(|v| v.as_ref())
    }
    
    pub fn get_wire_visibility(&self, wire: WireId) -> Option<Visibility> {
        let idx = wire.0 as usize;
        self.wire_visibility.get(idx).copied()
    }
}

/// Trait that backends must implement
/// Each backend (Clear, Yao, BGW, etc.) provides its own implementation
pub trait Backend {
    /// Execute a single instruction
    fn execute_instruction(&mut self, instruction: &Instruction, state: &mut VMState) -> Result<(), BackendError>;
    
    /// Get the name of this backend
    fn name(&self) -> &'static str;
}

/// Backend execution errors
#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Wire {0:?} not found or not set")]
    WireNotSet(WireId),
    
    #[error("Invalid visibility for wire {0:?}")]
    InvalidVisibility(WireId),
    
    #[error("Division by zero")]
    DivisionByZero,
    
    #[error("Arithmetic error: {0}")]
    ArithmeticError(String),
    
    #[error("Backend error: {0}")]
    BackendError(String),
}

