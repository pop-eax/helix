// Low-Level Intermediate Representation (LIR)
// Gate-level representation ready for backend compilation

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

// ============================================================================
// Top-level IR
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub metadata: Metadata,
    pub circuit: Circuit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub version: String,
    pub source_file: String,
    pub function_name: String,
    pub field_modulus: Option<u64>,  // For arithmetic circuits
    pub statistics: Statistics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    pub total_gates: usize,
    pub gate_counts: HashMap<GateType, usize>,
    pub circuit_depth: usize,
    pub num_inputs: usize,
    pub num_outputs: usize,
    pub num_wires: usize,
}

// ============================================================================
// Circuit Representation
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Circuit {
    pub inputs: Vec<Input>,
    pub gates: Vec<Gate>,
    pub outputs: Vec<WireId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub wire: WireId,
    /// Party ID - set to None during lowering, assigned by VM/executor at runtime
    pub party: Option<PartyId>,
    pub visibility: Visibility,
    pub name: Option<String>,  // For debugging
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WireId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartyId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Secret,
}

// ============================================================================
// Gates
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub id: GateId,
    pub gate_type: GateType,
    pub inputs: Vec<WireId>,
    pub output: WireId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GateId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GateType {
    // Boolean gates (for Yao/Garbled Circuits)
    And,
    Xor,
    Not,
    Or,   // Can be optimized to AND + XOR + NOT
    
    // Arithmetic gates (for BGW/Arithmetic MPC)
    Add,
    Mul,
    Sub,
    Div,  // Division in MPC - may require special handling
    Mod,  // Modulo in MPC - may require special handling
    
    // Constants (with field size for proper representation)
    Constant { value: u64, field_size: u64 },
    
    // Optimized variants (constant folding)
    AddConstant { constant: u64, field_size: u64 },
    MulConstant { constant: u64, field_size: u64 },
    SubConstant { constant: u64, field_size: u64 },

    // Comparison gates (output is 1-bit boolean in bit 0 of the output wire)
    /// Unsigned N-bit less-than: output bit 0 = 1 iff input1 < input2
    LessThan,
    /// N-bit equality: output bit 0 = 1 iff input1 == input2
    Equal,

    /// 3-input mux: inputs[0]=condition, inputs[1]=then_val, inputs[2]=else_val
    /// output = condition ? then_val : else_val
    Select,
}

// ============================================================================
// Serialization API
// ============================================================================

impl Program {
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }
    
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
    
}

// ============================================================================
// Builder API (for backends to construct circuits)
// ============================================================================

pub struct CircuitBuilder {
    next_wire: usize,
    next_gate: usize,
    inputs: Vec<Input>,
    gates: Vec<Gate>,
    outputs: Vec<WireId>,
}

impl CircuitBuilder {
    pub fn new() -> Self {
        Self {
            next_wire: 0,
            next_gate: 0,
            inputs: Vec::new(),
            gates: Vec::new(),
            outputs: Vec::new(),
        }
    }
    
    /// Add an input wire (party is set to None, to be assigned by VM/executor)
    pub fn add_input(&mut self, visibility: Visibility, name: Option<String>) -> WireId {
        let wire = WireId(self.next_wire);
        self.next_wire += 1;
        self.inputs.push(Input { wire, party: None, visibility, name });
        wire
    }
    
    /// Add a gate and return its output wire
    pub fn add_gate(&mut self, gate_type: GateType, inputs: Vec<WireId>) -> WireId {
        let output = WireId(self.next_wire);
        self.next_wire += 1;
        
        let gate = Gate {
            id: GateId(self.next_gate),
            gate_type,
            inputs,
            output,
        };
        self.next_gate += 1;
        self.gates.push(gate);
        
        output
    }
    
    /// Add a constant wire
    pub fn add_constant(&mut self, value: u64, field_size: u64) -> WireId {
        self.add_gate(GateType::Constant { value, field_size }, Vec::new())
    }
    
    /// Add an output wire
    pub fn add_output(&mut self, wire: WireId) {
        self.outputs.push(wire);
    }
    
    /// Get the current number of wires (useful for statistics)
    pub fn wire_count(&self) -> usize {
        self.next_wire
    }
    
    /// Get the current number of gates
    pub fn gate_count(&self) -> usize {
        self.gates.len()
    }
    
    pub fn build(mut self, mut metadata: Metadata) -> Program {
        let statistics = compute_statistics(&self.gates, &self.inputs, &self.outputs, self.next_wire);
        metadata.statistics = statistics;
        
        Program {
            metadata,
            circuit: Circuit {
                inputs: self.inputs,
                gates: self.gates,
                outputs: self.outputs,
            },
        }
    }
}

fn compute_statistics(
    gates: &[Gate],
    inputs: &[Input],
    outputs: &[WireId],
    num_wires: usize,
) -> Statistics {
    let mut gate_counts = HashMap::new();
    for gate in gates {
        *gate_counts.entry(gate.gate_type).or_insert(0) += 1;
    }

    let circuit_depth = compute_circuit_depth(gates, inputs, outputs);

    Statistics {
        total_gates: gates.len(),
        gate_counts,
        circuit_depth,
        num_inputs: inputs.len(),
        num_outputs: outputs.len(),
        num_wires,
    }
}

fn compute_circuit_depth(gates: &[Gate], inputs: &[Input], outputs: &[WireId]) -> usize {
    let mut depths: HashMap<WireId, usize> = HashMap::new();

    // Seed actual circuit input wires at depth 0.
    for input in inputs {
        depths.insert(input.wire, 0);
    }

    // Gates are emitted in topological (SSA) order by the builder, so a single
    // forward pass suffices.  Constant gates have no inputs and get depth 0.
    for gate in gates {
        let d = gate.inputs.iter()
            .filter_map(|w| depths.get(w))
            .max()
            .copied()
            .map(|d| d + 1)
            .unwrap_or(0);
        depths.insert(gate.output, d);
    }

    // Depth = longest path ending at any output wire.
    outputs.iter()
        .filter_map(|w| depths.get(w))
        .max()
        .copied()
        .unwrap_or(0)
}