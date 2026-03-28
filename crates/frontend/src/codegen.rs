// AST to HIR Code Generation

use std::collections::HashMap;
use crate::ast;
use crate::ast::*;
use ir::hir::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),
    
    #[error("Undefined struct: {0}")]
    UndefinedStruct(String),
    
    #[error("Undefined function: {0}")]
    UndefinedFunction(String),
    
    #[error("Invalid type conversion: {0}")]
    InvalidTypeConversion(String),
    
    #[error("Control flow error: {0}")]
    ControlFlowError(String),
}

pub type CodegenResult<T> = Result<T, CodegenError>;

/// Converts AST Program to HIR Program
pub fn codegen(ast: &Program) -> CodegenResult<HirProgram> {
    let mut generator = Codegen::new();
    generator.generate_program(ast)
}

struct Codegen {
    builder: HirBuilder,
    structs: HashMap<String, StructDef>,
    functions: HashMap<String, FunctionDef>,
    variables: Vec<HashMap<String, HirValue>>, // Stack of scopes
    current_function: Option<String>,
    /// Maps an array's representative HirValue to its element HirValues.
    /// Used to resolve arr[constant_idx] at codegen time without emitting ArrayLoad.
    arrays: HashMap<HirValue, Vec<HirValue>>,
    /// Maps a struct instance's sentinel HirValue to its field values by name.
    /// Used to resolve struct.field at codegen time without emitting StructField.
    struct_fields: HashMap<HirValue, HashMap<String, HirValue>>,
    /// > 0 when we are inlining a function call; prevents Return from emitting a terminator.
    inline_depth: usize,
    /// Captures the return value of an inlined function.
    inline_return: Option<HirValue>,
}

impl Codegen {
    fn new() -> Self {
        Self {
            builder: HirBuilder::new(),
            structs: HashMap::new(),
            functions: HashMap::new(),
            variables: Vec::new(),
            current_function: None,
            arrays: HashMap::new(),
            struct_fields: HashMap::new(),
            inline_depth: 0,
            inline_return: None,
        }
    }

    fn generate_program(&mut self, program: &ast::Program) -> CodegenResult<HirProgram> {
        // First pass: collect structs and functions
        for item in &program.items {
            match item {
                Item::StructDef(struct_def) => {
                    self.structs.insert(struct_def.name.clone(), struct_def.clone());
                }
                Item::FunctionDef(func_def) => {
                    self.functions.insert(func_def.name.clone(), func_def.clone());
                }
            }
        }

        // Convert structs
        let mut hir_structs = Vec::new();
        for item in &program.items {
            if let Item::StructDef(struct_def) = item {
                hir_structs.push(self.convert_struct_def(struct_def)?);
            }
        }

        // Convert functions
        let mut hir_functions = Vec::new();
        for item in &program.items {
            if let Item::FunctionDef(func_def) = item {
                hir_functions.push(self.convert_function(func_def)?);
            }
        }

        Ok(HirProgram {
            structs: hir_structs,
            functions: hir_functions,
        })
    }

    fn convert_struct_def(&self, struct_def: &StructDef) -> CodegenResult<HirStructDef> {
        let mut fields = Vec::new();
        for f in &struct_def.fields {
            fields.push(HirStructField {
                name: f.name.clone(),
                ty: self.convert_type(&f.ty)?,
            });
        }

        Ok(HirStructDef {
            name: struct_def.name.clone(),
            fields,
        })
    }

    fn convert_function(&mut self, func_def: &FunctionDef) -> CodegenResult<HirFunction> {
        self.current_function = Some(func_def.name.clone());
        self.builder = HirBuilder::new();
        self.enter_scope();

        // Create entry block
        let entry_block = self.builder.create_block();
        self.builder.set_current_block(entry_block);

        // Convert parameters — array and struct params are expanded to N scalar params.
        let mut hir_params = Vec::new();
        let mut param_idx = 0usize;
        self.arrays.clear();
        self.struct_fields.clear();
        for param in &func_def.params {
            let ty = self.convert_type(&param.ty)?;
            match ty {
                HirType::Array { ref element_type, size } => {
                    let base_idx = param_idx;
                    let mut elements = Vec::new();
                    for j in 0..size {
                        hir_params.push(HirParam {
                            name: format!("{}_{}", param.name, j),
                            ty: *element_type.clone(),
                            visibility: self.convert_visibility(&param.visibility),
                        });
                        elements.push(HirValue::Param(param_idx));
                        param_idx += 1;
                    }
                    let sentinel = HirValue::Param(base_idx);
                    self.arrays.insert(sentinel.clone(), elements);
                    self.declare_variable(param.name.clone(), sentinel);
                }
                HirType::Struct { ref name } => {
                    let struct_def = self.structs.get(name)
                        .ok_or_else(|| CodegenError::UndefinedStruct(name.clone()))?
                        .clone();
                    let base_idx = param_idx;
                    let mut fields = HashMap::new();
                    for field in &struct_def.fields {
                        let field_ty = self.convert_type(&field.ty)?;
                        hir_params.push(HirParam {
                            name: format!("{}_{}", param.name, field.name),
                            ty: field_ty,
                            visibility: self.convert_visibility(&param.visibility),
                        });
                        fields.insert(field.name.clone(), HirValue::Param(param_idx));
                        param_idx += 1;
                    }
                    let sentinel = HirValue::Param(base_idx);
                    self.struct_fields.insert(sentinel.clone(), fields);
                    self.declare_variable(param.name.clone(), sentinel);
                }
                _ => {
                    hir_params.push(HirParam {
                        name: param.name.clone(),
                        ty,
                        visibility: self.convert_visibility(&param.visibility),
                    });
                    self.declare_variable(param.name.clone(), HirValue::Param(param_idx));
                    param_idx += 1;
                }
            }
        }

        // Convert function body
        let return_type = self.convert_type(&func_def.return_type)?;
        self.convert_block(&func_def.body, entry_block)?;

        // Get all blocks (clone since get_blocks takes ownership)
        let blocks = self.builder.get_blocks_ref().clone();

        self.exit_scope();
        self.current_function = None;

        Ok(HirFunction {
            name: func_def.name.clone(),
            params: hir_params,
            return_type,
            entry_block,
            blocks,
        })
    }

    fn convert_block(&mut self, block: &Block, current_block: BlockId) -> CodegenResult<()> {
        self.builder.set_current_block(current_block);

        for stmt in &block.statements {
            self.convert_statement(stmt)?;
        }

        // If block doesn't have a terminator, add one
        // This will be handled by checking if terminator is set
        Ok(())
    }

    fn convert_statement(&mut self, stmt: &Statement) -> CodegenResult<()> {
        match stmt {
            Statement::VariableDecl(decl) => {
                self.convert_variable_decl(decl)?;
            }
            Statement::Assignment(assign) => {
                self.convert_assignment(assign)?;
            }
            Statement::If(if_stmt) => {
                self.convert_if_statement(if_stmt)?;
            }
            Statement::ForLoop(for_loop) => {
                self.convert_for_loop(for_loop)?;
            }
            Statement::Return(ret) => {
                self.convert_return(ret)?;
            }
            Statement::Directive(dir) => {
                self.convert_directive(dir)?;
            }
            Statement::Expr(expr) => {
                self.convert_expression(expr)?;
            }
        }
        Ok(())
    }

    fn convert_variable_decl(&mut self, decl: &VariableDecl) -> CodegenResult<()> {
        let ty = self.convert_type(&decl.ty)?;
        
        let value = if let Some(init) = &decl.initializer {
            self.convert_expression(init)?
        } else {
            // Default initialization - create zero/empty value
            match &ty {
                HirType::Field { size } => HirValue::Constant(HirConstant::Field { value: 0, size: *size }),
                HirType::Bool => HirValue::Constant(HirConstant::Bool(false)),
                HirType::Array { element_type, size } => {
                    // For arrays, we'll need to allocate
                    // For now, create a placeholder - this needs proper array allocation
                    return Err(CodegenError::InvalidTypeConversion(
                        "Array initialization without initializer not yet supported".to_string(),
                    ));
                }
                HirType::Struct { .. } => {
                    return Err(CodegenError::InvalidTypeConversion(
                        "Struct initialization without initializer not yet supported".to_string(),
                    ));
                }
            }
        };

        // Store the value directly
        // In a full SSA implementation, we'd create phi nodes for reassignments
        // For now, we store the value as-is (constants or instruction results)
        self.declare_variable(decl.name.clone(), value);
        Ok(())
    }

    fn convert_assignment(&mut self, assign: &Assignment) -> CodegenResult<()> {
        let value = self.convert_expression(&assign.value)?;
        
        // For now, we'll handle simple variable assignments
        // In a full SSA implementation, we'd need to handle phi nodes for reassignments
        if assign.lvalue.accesses.is_empty() {
            // Simple variable assignment
            if let Some(var_value) = self.lookup_variable(&assign.lvalue.base) {
                // Update the variable (in real SSA, this would create a new version)
                // For now, we'll just update the mapping
                if let Some(scope) = self.variables.last_mut() {
                    scope.insert(assign.lvalue.base.clone(), value);
                }
            } else {
                return Err(CodegenError::UndefinedVariable(assign.lvalue.base.clone()));
            }
        } else {
            // Array/struct field assignment - needs special handling
            return Err(CodegenError::InvalidTypeConversion(
                "Complex lvalue assignment not yet implemented".to_string(),
            ));
        }
        
        Ok(())
    }

    fn convert_if_statement(&mut self, if_stmt: &IfStatement) -> CodegenResult<()> {
        let condition = self.convert_expression(&if_stmt.condition)?;

        // Snapshot of variables visible before the branch.
        let pre_snap: HashMap<String, HirValue> = self.variables.last()
            .cloned()
            .unwrap_or_default();

        // ---- Compile then-branch in its own scope ----
        self.enter_scope();
        for stmt in &if_stmt.then_block.statements {
            self.convert_statement(stmt)?;
        }
        // Collect only variables that existed before the branch (outer accumulators).
        let then_vals: HashMap<String, HirValue> = self.variables.last()
            .map(|s| {
                s.iter()
                    .filter(|(k, _)| pre_snap.contains_key(*k))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.exit_scope();

        // ---- Compile else-branch in its own scope (parent sees pre-branch values) ----
        let else_vals: HashMap<String, HirValue> = if let Some(else_blk) = &if_stmt.else_block {
            self.enter_scope();
            for stmt in &else_blk.statements {
                self.convert_statement(stmt)?;
            }
            let vals = self.variables.last()
                .map(|s| {
                    s.iter()
                        .filter(|(k, _)| pre_snap.contains_key(*k))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default();
            self.exit_scope();
            vals
        } else {
            HashMap::new()
        };

        // ---- Merge: emit Select for every variable touched in either branch ----
        // Collect all names modified in at least one branch.
        let mut modified: std::collections::BTreeSet<String> = Default::default();
        for k in then_vals.keys() { modified.insert(k.clone()); }
        for k in else_vals.keys() { modified.insert(k.clone()); }

        for name in modified {
            let then_v = then_vals.get(&name)
                .or_else(|| pre_snap.get(&name))
                .cloned()
                .unwrap();
            let else_v = else_vals.get(&name)
                .or_else(|| pre_snap.get(&name))
                .cloned()
                .unwrap();

            // If both branches agree, no mux needed.
            let merged = if then_v == else_v {
                then_v
            } else {
                let id = self.builder.add_instruction(
                    HirInstructionKind::Select {
                        condition: condition.clone(),
                        then_val: then_v,
                        else_val: else_v,
                    },
                    HirType::Field { size: 64 },
                );
                HirValue::Instruction(id)
            };

            if let Some(scope) = self.variables.last_mut() {
                scope.insert(name, merged);
            }
        }

        Ok(())
    }

    fn convert_for_loop(&mut self, for_loop: &ForLoop) -> CodegenResult<()> {
        // Fully unroll: each iteration gets its own scope, and outer variable
        // assignments are propagated back to the enclosing scope.
        for i in for_loop.start..for_loop.end {
            self.enter_scope();
            self.declare_variable(
                for_loop.var_name.clone(),
                HirValue::Constant(HirConstant::Field { value: i, size: 64 }),
            );
            // Emit body statements into the current block without resetting it;
            // any nested if/else will leave the builder on the correct merge block.
            for stmt in &for_loop.body.statements {
                self.convert_statement(stmt)?;
            }
            // Propagate any assignments to pre-existing outer variables back up.
            self.propagate_scope_to_parent();
            self.exit_scope();
        }
        Ok(())
    }

    /// Copies updates from the innermost scope back to the parent scope,
    /// but only for variables that already existed in the parent (i.e. outer
    /// accumulators like `sum`). Loop-local declarations are NOT propagated.
    fn propagate_scope_to_parent(&mut self) {
        if self.variables.len() < 2 {
            return;
        }
        let len = self.variables.len();
        let updates: Vec<(String, HirValue)> = self.variables[len - 1]
            .iter()
            .filter(|(name, _)| self.variables[len - 2].contains_key(*name))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (name, value) in updates {
            self.variables[len - 2].insert(name, value);
        }
    }

    fn convert_return(&mut self, ret: &ReturnStatement) -> CodegenResult<()> {
        let value = self.convert_expression(&ret.value)?;
        if self.inline_depth > 0 {
            // Inlining: capture the return value; the caller will extract it.
            self.inline_return = Some(value);
        } else {
            if self.struct_fields.contains_key(&value) {
                return Err(CodegenError::InvalidTypeConversion(
                    "Functions that return a struct type can only be used via inlined calls; \
                     struct values cannot be lowered to a single circuit output wire"
                        .to_string(),
                ));
            }
            self.builder.set_terminator(HirTerminator::Return { value });
        }
        Ok(())
    }

    fn convert_directive(&mut self, dir: &Directive) -> CodegenResult<()> {
        match dir {
            Directive::Print(print) => {
                // Print directives are side effects - just evaluate expressions
                for expr in &print.expressions {
                    self.convert_expression(expr)?;
                }
            }
            Directive::Assert(assert) => {
                self.convert_expression(&assert.condition)?;
            }
            Directive::Abort(_) => {
                // Abort is a side effect - no codegen needed for now
            }
            Directive::Reveal(_) => {
                // Reveal is a side effect
            }
            Directive::Debug(debug) => {
                let debug_block = self.builder.create_block();
                self.convert_block(&debug.block, debug_block)?;
            }
        }
        Ok(())
    }

    fn convert_expression(&mut self, expr: &Expression) -> CodegenResult<HirValue> {
        let base_value = match &expr.base {
            ExpressionBase::Literal(lit) => self.convert_literal(lit)?,
            ExpressionBase::ArrayLiteral(arr) => self.convert_array_literal(arr)?,
            ExpressionBase::StructLiteral(struct_lit) => self.convert_struct_literal(struct_lit)?,
            ExpressionBase::FunctionCall(call) => self.convert_function_call(call)?,
            ExpressionBase::Identifier(name) => {
                self.lookup_variable(name)
                    .ok_or_else(|| CodegenError::UndefinedVariable(name.clone()))?
                    .clone()
            }
            ExpressionBase::BinaryOp(bin_op) => self.convert_binary_op(bin_op)?,
            ExpressionBase::UnaryOp(unary_op) => self.convert_unary_op(unary_op)?,
            ExpressionBase::Parenthesized(expr) => self.convert_expression(expr)?,
        };

        // Apply postfix accesses
        let mut result = base_value;
        for access in &expr.accesses {
            result = self.apply_access(&result, access)?;
        }

        Ok(result)
    }

    fn convert_literal(&self, lit: &Literal) -> CodegenResult<HirValue> {
        match lit {
            Literal::Integer(value) => Ok(HirValue::Constant(HirConstant::Field {
                value: *value,
                size: 64, // Default size - should be inferred from context
            })),
            Literal::Bool(value) => Ok(HirValue::Constant(HirConstant::Bool(*value))),
        }
    }

    fn convert_array_literal(&mut self, arr: &ArrayLiteral) -> CodegenResult<HirValue> {
        if arr.elements.is_empty() {
            return Err(CodegenError::InvalidTypeConversion(
                "Empty array literals not supported".to_string(),
            ));
        }

        let mut elements = Vec::new();
        for elem in &arr.elements {
            elements.push(self.convert_expression(elem)?);
        }

        // Same sentinel approach as struct literals: reserve a value ID without
        // emitting an instruction, store all elements in the arrays map.
        let sentinel_id = self.builder.new_value_id();
        let sentinel = HirValue::Instruction(sentinel_id);
        self.arrays.insert(sentinel.clone(), elements);
        Ok(sentinel)
    }

    fn convert_struct_literal(&mut self, struct_lit: &StructLiteral) -> CodegenResult<HirValue> {
        // Convert field values in definition order.
        let struct_def = self.structs.get(&struct_lit.struct_name)
            .ok_or_else(|| CodegenError::UndefinedStruct(struct_lit.struct_name.clone()))?
            .clone();

        let mut fields = HashMap::new();
        for field_def in &struct_def.fields {
            let init = struct_lit.fields.iter()
                .find(|fi| fi.name == field_def.name)
                .ok_or_else(|| CodegenError::InvalidTypeConversion(
                    format!("Missing field '{}' in struct literal '{}'", field_def.name, struct_lit.struct_name)
                ))?;
            let value = self.convert_expression(&init.value)?;
            fields.insert(field_def.name.clone(), value);
        }

        // Reserve a unique value ID as a sentinel (no instruction emitted — the sentinel
        // never appears in the HIR instruction list, so the lowering won't process it).
        let sentinel_id = self.builder.new_value_id();
        let sentinel = HirValue::Instruction(sentinel_id);
        self.struct_fields.insert(sentinel.clone(), fields);
        Ok(sentinel)
    }

    fn convert_function_call(&mut self, call: &FunctionCall) -> CodegenResult<HirValue> {
        let func_def = self.functions.get(&call.name)
            .ok_or_else(|| CodegenError::UndefinedFunction(call.name.clone()))?
            .clone();

        // Evaluate arguments in the caller's scope before entering the callee's scope.
        let mut args = Vec::new();
        for arg in &call.arguments {
            args.push(self.convert_expression(arg)?);
        }

        // Inline the callee: bind params to args in a fresh scope, compile the body,
        // capture the return value instead of emitting a terminator.
        self.enter_scope();
        self.inline_depth += 1;

        for (param, arg) in func_def.params.iter().zip(args.iter()) {
            // For array/struct params the arg is already a sentinel with its map entry
            // registered; just bind the param name to the same sentinel.
            self.declare_variable(param.name.clone(), arg.clone());
        }

        for stmt in &func_def.body.statements {
            self.convert_statement(stmt)?;
            if self.inline_return.is_some() {
                break;
            }
        }

        self.inline_depth -= 1;
        self.exit_scope();

        self.inline_return.take().ok_or_else(|| CodegenError::ControlFlowError(
            format!("Inlined function '{}' has no reachable return statement", call.name)
        ))
    }

    fn convert_binary_op(&mut self, bin_op: &BinaryOp) -> CodegenResult<HirValue> {
        match bin_op {
            BinaryOp::Add(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Field { size: 64 }; // Infer from types
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::Add { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::Subtract(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::Sub { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::Multiply(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::Mul { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::Divide(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::Div { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::Modulo(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::Mod { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::BitwiseAnd(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::BitwiseAnd { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::BitwiseOr(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::BitwiseOr { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::BitwiseXor(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::BitwiseXor { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::Shift(op, left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Field { size: 64 };
                let kind = match op {
                    ShiftOp::Left => HirInstructionKind::ShiftLeft {
                        value: left_val,
                        amount: right_val,
                    },
                    ShiftOp::Right => HirInstructionKind::ShiftRight {
                        value: left_val,
                        amount: right_val,
                    },
                };
                let value_id = self.builder.add_instruction(kind, ty);
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::LogicalAnd(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Bool;
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::LogicalAnd { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::LogicalOr(left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Bool;
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::LogicalOr { left: left_val, right: right_val },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            BinaryOp::Comparison(op, left, right) => {
                let left_val = self.convert_expression(left)?;
                let right_val = self.convert_expression(right)?;
                let ty = HirType::Bool;
                let kind = match op {
                    ComparisonOp::Equal => HirInstructionKind::Equal {
                        left: left_val,
                        right: right_val,
                    },
                    ComparisonOp::NotEqual => HirInstructionKind::NotEqual {
                        left: left_val,
                        right: right_val,
                    },
                    ComparisonOp::LessThan => HirInstructionKind::LessThan {
                        left: left_val,
                        right: right_val,
                    },
                    ComparisonOp::LessThanOrEqual => HirInstructionKind::LessThanOrEqual {
                        left: left_val,
                        right: right_val,
                    },
                    ComparisonOp::GreaterThan => HirInstructionKind::GreaterThan {
                        left: left_val,
                        right: right_val,
                    },
                    ComparisonOp::GreaterThanOrEqual => HirInstructionKind::GreaterThanOrEqual {
                        left: left_val,
                        right: right_val,
                    },
                };
                let value_id = self.builder.add_instruction(kind, ty);
                Ok(HirValue::Instruction(value_id))
            }
        }
    }

    fn convert_unary_op(&mut self, unary_op: &UnaryOp) -> CodegenResult<HirValue> {
        match unary_op {
            UnaryOp::Negate(expr) => {
                let value = self.convert_expression(expr)?;
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::Negate { value },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            UnaryOp::Not(expr) => {
                let value = self.convert_expression(expr)?;
                let ty = HirType::Bool;
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::Not { value },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            UnaryOp::BitwiseNot(expr) => {
                let value = self.convert_expression(expr)?;
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::BitwiseNot { value },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
        }
    }

    fn apply_access(&mut self, base: &HirValue, access: &ExprAccess) -> CodegenResult<HirValue> {
        match access {
            ExprAccess::Array(index_expr) => {
                let index = self.convert_expression(index_expr)?;
                // If this array is tracked in the codegen map, resolve at compile time.
                if let Some(elements) = self.arrays.get(base).cloned() {
                    return match &index {
                        HirValue::Constant(HirConstant::Field { value: idx, .. }) => {
                            elements.get(*idx as usize).cloned().ok_or_else(|| {
                                CodegenError::InvalidTypeConversion(format!(
                                    "Array index {} out of bounds (size {})",
                                    idx,
                                    elements.len()
                                ))
                            })
                        }
                        _ => Err(CodegenError::InvalidTypeConversion(
                            "Dynamic array indexing not supported — \
                             use compile-time constants or for-loops with constant bounds"
                                .to_string(),
                        )),
                    };
                }
                // Fallback: emit ArrayLoad (will error at lowering for unsupported cases).
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::ArrayLoad {
                        array: base.clone(),
                        index,
                    },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
            ExprAccess::Field(field_name) => {
                // If this struct instance is tracked in the codegen map, resolve at compile time.
                if let Some(fields) = self.struct_fields.get(base).cloned() {
                    return fields.get(field_name).cloned()
                        .ok_or_else(|| CodegenError::InvalidTypeConversion(
                            format!("Struct has no field '{}'", field_name)
                        ));
                }
                // Fallback: emit StructField (will error at lowering for untracked structs).
                let ty = HirType::Field { size: 64 };
                let value_id = self.builder.add_instruction(
                    HirInstructionKind::StructField {
                        struct_val: base.clone(),
                        field_name: field_name.clone(),
                    },
                    ty,
                );
                Ok(HirValue::Instruction(value_id))
            }
        }
    }

    // Type conversion helpers
    fn convert_type(&self, ty: &TypeExpr) -> CodegenResult<HirType> {
        match ty {
            TypeExpr::Base(base) => match base {
                BaseType::Field(field) => Ok(HirType::Field { size: field.size }),
                BaseType::Bool => Ok(HirType::Bool),
            },
            TypeExpr::Array(arr) => Ok(HirType::Array {
                element_type: Box::new(self.convert_type(&arr.element_type)?),
                size: arr.size,
            }),
            TypeExpr::Struct(struct_ty) => Ok(HirType::Struct {
                name: struct_ty.name.clone(),
            }),
        }
    }

    fn convert_visibility(&self, vis: &crate::ast::Visibility) -> ir::hir::Visibility {
        match vis {
            crate::ast::Visibility::Public => ir::hir::Visibility::Public,
            crate::ast::Visibility::Secret => ir::hir::Visibility::Secret,
        }
    }

    // Scope management
    fn enter_scope(&mut self) {
        self.variables.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.variables.pop();
    }

    fn declare_variable(&mut self, name: String, value: HirValue) {
        if let Some(scope) = self.variables.last_mut() {
            scope.insert(name, value);
        }
    }

    fn lookup_variable(&self, name: &str) -> Option<&HirValue> {
        for scope in self.variables.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::*;
    use crate::type_checker::*;

    fn parse_type_check_and_codegen(source: &str) -> Result<ir::hir::HirProgram, CodegenError> {
        let pairs = parse_program(source).unwrap();
        let program = build_ast(pairs).unwrap();
        type_check(&program).unwrap();
        codegen(&program)
    }

    #[test]
    fn test_codegen_simple_function() {
        let source = "fn add(Public Field<64> a, Public Field<64> b) -> Field<64> { return a + b; }";
        let result = parse_type_check_and_codegen(source);
        assert!(result.is_ok());
        
        let hir = result.unwrap();
        assert_eq!(hir.functions.len(), 1);
        assert_eq!(hir.functions[0].name, "add");
    }

    #[test]
    fn test_codegen_arithmetic_operations() {
        let source = "fn compute(Public Field<64> a, Public Field<64> b) -> Field<64> { return a + b * 2; }";
        let result = parse_type_check_and_codegen(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_codegen_conditional() {
        let source = "fn max(Public Field<64> a, Public Field<64> b) -> Field<64> { if a > b { return a; } else { return b; } }";
        let result = parse_type_check_and_codegen(source);
        assert!(result.is_ok());
    }
}

