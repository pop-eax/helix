// High-Level Intermediate Representation (HIR)
// This is closer to the AST but structured for optimization and lowering to LIR

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

// ============================================================================
// Top-level HIR
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirProgram {
    pub structs: Vec<HirStructDef>,
    pub functions: Vec<HirFunction>,
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HirType {
    Field { size: u64 },
    Bool,
    Array { element_type: Box<HirType>, size: u64 },
    Struct { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Secret,
}

// ============================================================================
// Structs
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirStructDef {
    pub name: String,
    pub fields: Vec<HirStructField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirStructField {
    pub name: String,
    pub ty: HirType,
}

// ============================================================================
// Functions
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<HirParam>,
    pub return_type: HirType,
    pub entry_block: BlockId,
    pub blocks: HashMap<BlockId, HirBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirParam {
    pub name: String,
    pub ty: HirType,
    pub visibility: Visibility,
}

// ============================================================================
// Basic Blocks and Control Flow
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirBlock {
    pub id: BlockId,
    pub instructions: Vec<HirInstruction>,
    pub terminator: HirTerminator,
}

// ============================================================================
// Values (SSA-like)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HirValue {
    /// A value produced by an instruction
    Instruction(ValueId),
    /// A parameter
    Param(usize), // Index into function params
    /// A constant
    Constant(HirConstant),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValueId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HirConstant {
    Field { value: u64, size: u64 },
    Bool(bool),
}

// ============================================================================
// Instructions
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirInstruction {
    pub id: ValueId,
    pub kind: HirInstructionKind,
    pub ty: HirType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HirInstructionKind {
    // Binary operations
    Add { left: HirValue, right: HirValue },
    Sub { left: HirValue, right: HirValue },
    Mul { left: HirValue, right: HirValue },
    Div { left: HirValue, right: HirValue },
    Mod { left: HirValue, right: HirValue },
    
    // Bitwise operations
    BitwiseAnd { left: HirValue, right: HirValue },
    BitwiseOr { left: HirValue, right: HirValue },
    BitwiseXor { left: HirValue, right: HirValue },
    ShiftLeft { value: HirValue, amount: HirValue },
    ShiftRight { value: HirValue, amount: HirValue },
    
    // Logical operations
    LogicalAnd { left: HirValue, right: HirValue },
    LogicalOr { left: HirValue, right: HirValue },
    
    // Comparisons
    Equal { left: HirValue, right: HirValue },
    NotEqual { left: HirValue, right: HirValue },
    LessThan { left: HirValue, right: HirValue },
    LessThanOrEqual { left: HirValue, right: HirValue },
    GreaterThan { left: HirValue, right: HirValue },
    GreaterThanOrEqual { left: HirValue, right: HirValue },
    
    // Unary operations
    Negate { value: HirValue },
    Not { value: HirValue },
    BitwiseNot { value: HirValue },
    
    // Memory/Array operations
    ArrayLoad { array: HirValue, index: HirValue },
    ArrayStore { array: HirValue, index: HirValue, value: HirValue },
    ArrayAlloc { element_type: HirType, size: u64 },
    
    // Struct operations
    StructField { struct_val: HirValue, field_name: String },
    StructAlloc { struct_name: String },
    
    // Function calls
    Call { function_name: String, args: Vec<HirValue> },
    
    // Phi node for SSA (used in control flow merging)
    Phi { incoming: Vec<(BlockId, HirValue)> },
}

// ============================================================================
// Terminators (control flow)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HirTerminator {
    /// Return a value
    Return { value: HirValue },
    /// Conditional branch
    Branch { condition: HirValue, then_block: BlockId, else_block: BlockId },
    /// Unconditional jump
    Jump { target: BlockId },
    /// Loop: jump to header with condition (for future use)
    /// Note: MPC circuits are typically acyclic, so loops are unrolled
    Loop { header: BlockId, condition: HirValue, body: BlockId, exit: BlockId },
    
    /// Unreachable (for blocks that should never be reached)
    Unreachable,
}

// ============================================================================
// HIR Builder (for constructing HIR from AST)
// ============================================================================

pub struct HirBuilder {
    next_value_id: usize,
    next_block_id: usize,
    current_function: Option<String>,
    blocks: HashMap<BlockId, HirBlock>,
    current_block: Option<BlockId>,
}

impl HirBuilder {
    pub fn new() -> Self {
        Self {
            next_value_id: 0,
            next_block_id: 0,
            current_function: None,
            blocks: HashMap::new(),
            current_block: None,
        }
    }
    
    pub fn new_value_id(&mut self) -> ValueId {
        let id = ValueId(self.next_value_id);
        self.next_value_id += 1;
        id
    }
    
    pub fn new_block_id(&mut self) -> BlockId {
        let id = BlockId(self.next_block_id);
        self.next_block_id += 1;
        id
    }
    
    pub fn create_block(&mut self) -> BlockId {
        let id = self.new_block_id();
        self.blocks.insert(id, HirBlock {
            id,
            instructions: Vec::new(),
            terminator: HirTerminator::Jump { target: id }, // Placeholder
        });
        id
    }
    
    pub fn set_current_block(&mut self, block: BlockId) {
        self.current_block = Some(block);
    }
    
    pub fn add_instruction(&mut self, kind: HirInstructionKind, ty: HirType) -> ValueId {
        let id = self.new_value_id();
        let instruction = HirInstruction { id, kind, ty };
        
        if let Some(block_id) = self.current_block {
            if let Some(block) = self.blocks.get_mut(&block_id) {
                block.instructions.push(instruction);
            }
        }
        
        id
    }
    
    pub fn set_terminator(&mut self, terminator: HirTerminator) {
        if let Some(block_id) = self.current_block {
            if let Some(block) = self.blocks.get_mut(&block_id) {
                block.terminator = terminator;
            }
        }
    }
    
    pub fn get_blocks(self) -> HashMap<BlockId, HirBlock> {
        self.blocks
    }
    
    pub fn get_blocks_ref(&self) -> &HashMap<BlockId, HirBlock> {
        &self.blocks
    }
}

