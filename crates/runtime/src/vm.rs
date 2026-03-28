use ir::lir::{WireId, Visibility};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityPair {
    pub left: Visibility,
    pub right: Visibility,
}

impl VisibilityPair {
    pub fn new(left: Visibility, right: Visibility) -> Self {
        Self { left, right }
    }
    
    pub fn output_visibility(&self) -> Visibility {
        match (self.left, self.right) {
            (Visibility::Secret, _) | (_, Visibility::Secret) => Visibility::Secret,
            _ => Visibility::Public,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Instruction {
    // Boolean gates
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
    Or {
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
    
    // Comparison gates (output is 1-bit boolean stored in bit 0)
    LessThan {
        vis: VisibilityPair,
        input1: WireId,
        input2: WireId,
        output: WireId,
    },
    Equal {
        vis: VisibilityPair,
        input1: WireId,
        input2: WireId,
        output: WireId,
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

    /// Set an input wire value
    /// Each backend can handle this differently (e.g., create labels, set shares, etc.)
    fn set_input(
        &mut self,
        wire: WireId,
        value: u64,
        visibility: Visibility,
        state: &mut VMState,
    ) -> Result<(), BackendError>;
    
    /// Get an output wire value
    /// Each backend can handle this differently (e.g., read from state, evaluate circuit, etc.)
    fn get_output(
        &mut self,
        wire: WireId,
        state: &VMState,
    ) -> Result<u64, BackendError>;

    /// Returns pending outgoing messages (party_id, payload) and clears the internal queue.
    /// Backends that require network communication between rounds should override this.
    fn take_outgoing(&mut self) -> Vec<(usize, Vec<u8>)> { vec![] }

    /// Delivers incoming messages from other parties so execution can resume.
    fn receive_replies(&mut self, _messages: Vec<(usize, Vec<u8>)>) -> Result<(), BackendError> { Ok(()) }

    /// Called when THIS party is the owner of `wire`.
    ///
    /// Returns a `Vec` of length `n_parties` where element `i` is the serialized
    /// share that should be sent to party `i`.  The default broadcasts the raw
    /// 8-byte little-endian encoding of `value` to every party (suitable for
    /// the clear backend where all parties see the same value).
    fn share_input(
        &mut self,
        _wire: WireId,
        value: u64,
        n_parties: usize,
    ) -> Result<Vec<Vec<u8>>, BackendError> {
        Ok(vec![value.to_le_bytes().to_vec(); n_parties])
    }

    /// Called once after all gate instructions have run, before output values
    /// are collected.  Backends that need a network round for output
    /// reconstruction (e.g. BGW) should queue outgoing share messages here;
    /// the runner will deliver the replies via [`receive_replies`] and then
    /// call [`get_output`].  The default is a no-op.
    fn prepare_output_reconstruction(
        &mut self,
        _wires: &[WireId],
        _state: &VMState,
    ) -> Result<(), BackendError> {
        Ok(())
    }

    /// Called to deliver this party's share of `wire` (owned by another party,
    /// or by self after splitting via [`share_input`]).
    ///
    /// The default interprets the share as a little-endian u64 and stores it as
    /// a `Clear` wire value — sufficient for the clear backend.
    fn receive_input_share(
        &mut self,
        wire: WireId,
        visibility: Visibility,
        share: Vec<u8>,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        let bytes: [u8; 8] = share.try_into().map_err(|_| {
            BackendError::BackendError(
                "default receive_input_share expects an 8-byte share".into(),
            )
        })?;
        state.set_wire(wire, WireValue::Clear(u64::from_le_bytes(bytes)), visibility);
        Ok(())
    }
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

