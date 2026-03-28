// Display utilities for HIR

use crate::hir::*;

pub fn display_hir_program(program: &HirProgram) -> String {
    let mut result = String::new();
    
    // Display structs
    for struct_def in &program.structs {
        result.push_str(&display_hir_struct(struct_def));
        result.push_str("\n\n");
    }
    
    // Display functions
    for function in &program.functions {
        result.push_str(&display_hir_function(function));
        result.push_str("\n\n");
    }
    
    result
}

fn display_hir_struct(struct_def: &HirStructDef) -> String {
    let mut result = format!("struct {} {{\n", struct_def.name);
    for field in &struct_def.fields {
        result.push_str(&format!("    {}: {},\n", field.name, display_hir_type(&field.ty)));
    }
    result.push_str("}");
    result
}

fn display_hir_function(function: &HirFunction) -> String {
    let mut result = format!("fn {}(", function.name);
    
    for (i, param) in function.params.iter().enumerate() {
        if i > 0 {
            result.push_str(", ");
        }
        result.push_str(&format!(
            "{} {} {}",
            display_visibility(param.visibility),
            display_hir_type(&param.ty),
            param.name
        ));
    }
    
    result.push_str(&format!(") -> {} {{\n", display_hir_type(&function.return_type)));
    
    // Display blocks
    if let Some(entry_block) = function.blocks.get(&function.entry_block) {
        result.push_str(&display_hir_block(entry_block, &function.blocks, 1));
    }
    
    result.push_str("}");
    result
}

fn display_hir_block(block: &HirBlock, all_blocks: &std::collections::HashMap<BlockId, HirBlock>, indent: usize) -> String {
    let indent_str = "    ".repeat(indent);
    let mut result = format!("{}block {:?}:\n", indent_str, block.id);
    
    // Display instructions
    for instruction in &block.instructions {
        result.push_str(&format!("{}{}\n", indent_str, display_hir_instruction(instruction)));
    }
    
    // Display terminator
    result.push_str(&format!("{}{}\n", indent_str, display_hir_terminator(&block.terminator, all_blocks)));
    
    result
}

fn display_hir_instruction(instruction: &HirInstruction) -> String {
    format!(
        "%{} = {} : {}",
        instruction.id.0,
        display_hir_instruction_kind(&instruction.kind),
        display_hir_type(&instruction.ty)
    )
}

fn display_hir_instruction_kind(kind: &HirInstructionKind) -> String {
    match kind {
        HirInstructionKind::Add { left, right } => format!("add({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::Sub { left, right } => format!("sub({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::Mul { left, right } => format!("mul({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::Div { left, right } => format!("div({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::Mod { left, right } => format!("mod({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::BitwiseAnd { left, right } => format!("and({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::BitwiseOr { left, right } => format!("or({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::BitwiseXor { left, right } => format!("xor({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::ShiftLeft { value, amount } => format!("shl({}, {})", display_hir_value(value), display_hir_value(amount)),
        HirInstructionKind::ShiftRight { value, amount } => format!("shr({}, {})", display_hir_value(value), display_hir_value(amount)),
        HirInstructionKind::LogicalAnd { left, right } => format!("logical_and({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::LogicalOr { left, right } => format!("logical_or({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::Equal { left, right } => format!("eq({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::NotEqual { left, right } => format!("ne({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::LessThan { left, right } => format!("lt({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::LessThanOrEqual { left, right } => format!("le({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::GreaterThan { left, right } => format!("gt({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::GreaterThanOrEqual { left, right } => format!("ge({}, {})", display_hir_value(left), display_hir_value(right)),
        HirInstructionKind::Negate { value } => format!("neg({})", display_hir_value(value)),
        HirInstructionKind::Not { value } => format!("not({})", display_hir_value(value)),
        HirInstructionKind::BitwiseNot { value } => format!("bitwise_not({})", display_hir_value(value)),
        HirInstructionKind::ArrayLoad { array, index } => format!("array_load({}, {})", display_hir_value(array), display_hir_value(index)),
        HirInstructionKind::ArrayStore { array, index, value } => format!("array_store({}, {}, {})", display_hir_value(array), display_hir_value(index), display_hir_value(value)),
        HirInstructionKind::ArrayAlloc { element_type, size } => format!("array_alloc({}, {})", display_hir_type(element_type), size),
        HirInstructionKind::StructField { struct_val, field_name } => format!("struct_field({}, {})", display_hir_value(struct_val), field_name),
        HirInstructionKind::StructAlloc { struct_name } => format!("struct_alloc({})", struct_name),
        HirInstructionKind::Call { function_name, args } => {
            let args_str: Vec<String> = args.iter().map(display_hir_value).collect();
            format!("call({}, [{}])", function_name, args_str.join(", "))
        }
        HirInstructionKind::Phi { incoming } => {
            let incoming_str: Vec<String> = incoming.iter()
                .map(|(block, val)| format!("{:?}: {}", block, display_hir_value(val)))
                .collect();
            format!("phi([{}])", incoming_str.join(", "))
        }
        HirInstructionKind::Select { condition, then_val, else_val } => {
            format!("select({}, {}, {})",
                display_hir_value(condition),
                display_hir_value(then_val),
                display_hir_value(else_val))
        }
    }
}

fn display_hir_terminator(terminator: &HirTerminator, _all_blocks: &std::collections::HashMap<BlockId, HirBlock>) -> String {
    match terminator {
        HirTerminator::Return { value } => format!("return {}", display_hir_value(value)),
        HirTerminator::Branch { condition, then_block, else_block } => {
            format!("br({}, {:?}, {:?})", display_hir_value(condition), then_block, else_block)
        }
        HirTerminator::Jump { target } => format!("jump {:?}", target),
        HirTerminator::Loop { header, condition, body, exit } => {
            format!("loop(header: {:?}, cond: {}, body: {:?}, exit: {:?})", 
                header, display_hir_value(condition), body, exit)
        }
        HirTerminator::Unreachable => "unreachable".to_string(),
    }
}

fn display_hir_value(value: &HirValue) -> String {
    match value {
        HirValue::Instruction(id) => format!("%{}", id.0),
        HirValue::Param(idx) => format!("param[{}]", idx),
        HirValue::Constant(constant) => display_hir_constant(*constant),
    }
}

fn display_hir_constant(constant: HirConstant) -> String {
    match constant {
        HirConstant::Field { value, size } => format!("{}:Field<{}>", value, size),
        HirConstant::Bool(true) => "true".to_string(),
        HirConstant::Bool(false) => "false".to_string(),
    }
}

fn display_hir_type(ty: &HirType) -> String {
    match ty {
        HirType::Field { size } => format!("Field<{}>", size),
        HirType::Bool => "Bool".to_string(),
        HirType::Array { element_type, size } => {
            format!("Array<{}, {}>", display_hir_type(element_type), size)
        }
        HirType::Struct { name } => name.clone(),
    }
}

fn display_visibility(vis: Visibility) -> &'static str {
    match vis {
        Visibility::Public => "Public",
        Visibility::Secret => "Secret",
    }
}

