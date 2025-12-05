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
    pub party: PartyId,
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
    
    /// Add an input wire from a party
    pub fn add_input(&mut self, party: PartyId, visibility: Visibility, name: Option<String>) -> WireId {
        let wire = WireId(self.next_wire);
        self.next_wire += 1;
        self.inputs.push(Input { wire, party, visibility, name });
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
        let statistics = compute_statistics(&self.gates, self.inputs.len(), self.outputs.len(), self.next_wire);
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
    num_inputs: usize,
    num_outputs: usize,
    num_wires: usize,
) -> Statistics {
    let mut gate_counts = HashMap::new();
    for gate in gates {
        *gate_counts.entry(gate.gate_type).or_insert(0) += 1;
    }
    
    // Compute depth (requires topological analysis)
    let circuit_depth = compute_circuit_depth(gates);
    
    Statistics {
        total_gates: gates.len(),
        gate_counts,
        circuit_depth,
        num_inputs,
        num_outputs,
        num_wires,
    }
}

fn compute_circuit_depth(gates: &[Gate]) -> usize {
    // Build dependency graph and compute longest path
    // Inputs have depth 0, gates have depth = max(input_depths) + 1
    
    // First, collect all input wires (wires that are never outputs of gates)
    let mut all_wires: std::collections::HashSet<WireId> = gates.iter()
        .flat_map(|g| g.inputs.iter().copied())
        .collect();
    let output_wires: std::collections::HashSet<WireId> = gates.iter()
        .map(|g| g.output)
        .collect();
    
    // Input wires are those that appear as inputs but never as outputs
    let input_wires: std::collections::HashSet<WireId> = all_wires
        .difference(&output_wires)
        .copied()
        .collect();
    
    let mut depths = HashMap::new();
    
    // Initialize input wires to depth 0
    for wire in &input_wires {
        depths.insert(*wire, 0);
    }
    
    // Process gates (assuming topological order, but handle cycles gracefully)
    for gate in gates {
        let max_input_depth = gate.inputs.iter()
            .filter_map(|w| depths.get(w))
            .max()
            .unwrap_or(&0);
        depths.insert(gate.output, max_input_depth + 1);
    }
    
    depths.values().max().copied().unwrap_or(0)
}