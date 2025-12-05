use std::collections::HashMap;

use crate::ast::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),
    
    #[error("Undefined struct: {0}")]
    UndefinedStruct(String),
    
    #[error("Undefined function: {0}")]
    UndefinedFunction(String),
    
    #[error("Type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    
    #[error("Invalid field access: struct {struct_name} does not have field {field_name}")]
    InvalidFieldAccess { struct_name: String, field_name: String },
    
    #[error("Array access on non-array type: {0}")]
    InvalidArrayAccess(String),
    
    #[error("Array index must be an integer, found: {0}")]
    InvalidArrayIndex(String),
    
    #[error("Function call argument count mismatch: expected {expected}, found {found}")]
    ArgumentCountMismatch { expected: usize, found: usize },
    
    #[error("Function call argument type mismatch at position {position}: expected {expected}, found {found}")]
    ArgumentTypeMismatch {
        position: usize,
        expected: String,
        found: String,
    },
    
    #[error("Return type mismatch: expected {expected}, found {found}")]
    ReturnTypeMismatch { expected: String, found: String },
    
    #[error("Missing return statement in function")]
    MissingReturn,
    
    #[error("Invalid operator for type: {op} on {ty}")]
    InvalidOperator { op: String, ty: String },
    
    #[error("Invalid unary operator: {op} on {ty}")]
    InvalidUnaryOperator { op: String, ty: String },
    
    #[error("Array literal element type mismatch: expected {expected}, found {found}")]
    ArrayElementTypeMismatch { expected: String, found: String },
    
    #[error("Struct literal field missing: {field_name} in {struct_name}")]
    MissingStructField { struct_name: String, field_name: String },
    
    #[error("Struct literal extra field: {field_name} in {struct_name}")]
    ExtraStructField { struct_name: String, field_name: String },
    
    #[error("Variable already declared in scope: {0}")]
    VariableAlreadyDeclared(String),
    
    #[error("Struct already defined: {0}")]
    StructAlreadyDefined(String),
    
    #[error("Function already defined: {0}")]
    FunctionAlreadyDefined(String),
}

pub type TypeCheckResult<T> = Result<T, TypeError>;

#[derive(Debug, Clone)]
struct VariableInfo {
    ty: TypeExpr,
    visibility: Visibility,
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    params: Vec<(TypeExpr, Visibility)>,
    return_type: TypeExpr,
}

pub struct TypeChecker {
    structs: HashMap<String, StructDef>,
    functions: HashMap<String, FunctionSignature>,
    variables: Vec<HashMap<String, VariableInfo>>,
    current_return_type: Option<TypeExpr>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            structs: HashMap::new(),
            functions: HashMap::new(),
            variables: Vec::new(),
            current_return_type: None,
        }
    }

    pub fn check_program(&mut self, program: &Program) -> TypeCheckResult<()> {
        // First pass: collect struct and function definitions
        for item in &program.items {
            match item {
                Item::StructDef(struct_def) => {
                    if self.structs.contains_key(&struct_def.name) {
                        return Err(TypeError::StructAlreadyDefined(struct_def.name.clone()));
                    }
                    self.structs.insert(struct_def.name.clone(), struct_def.clone());
                }
                Item::FunctionDef(func_def) => {
                    if self.functions.contains_key(&func_def.name) {
                        return Err(TypeError::FunctionAlreadyDefined(func_def.name.clone()));
                    }
                    let signature = FunctionSignature {
                        params: func_def.params.iter().map(|p| (p.ty.clone(), p.visibility.clone())).collect(),
                        return_type: func_def.return_type.clone(),
                    };
                    self.functions.insert(func_def.name.clone(), signature);
                }
            }
        }

        // Second pass: validate struct field types and check function bodies
        for item in &program.items {
            match item {
                Item::StructDef(struct_def) => {
                    self.check_struct_def(struct_def)?;
                }
                Item::FunctionDef(func_def) => {
                    self.check_function_def(func_def)?;
                }
            }
        }

        Ok(())
    }

    fn check_struct_def(&mut self, struct_def: &StructDef) -> TypeCheckResult<()> {
        // Check that all field types are valid
        for field in &struct_def.fields {
            self.check_type_expr(&field.ty)?;
        }
        Ok(())
    }

    fn check_function_def(&mut self, func_def: &FunctionDef) -> TypeCheckResult<()> {
        // Check return type is valid
        self.check_type_expr(&func_def.return_type)?;

        // Enter function scope
        self.enter_scope();
        self.current_return_type = Some(func_def.return_type.clone());

        // Add parameters to scope
        for param in &func_def.params {
            self.check_type_expr(&param.ty)?;
            if self.lookup_variable(&param.name).is_some() {
                return Err(TypeError::VariableAlreadyDeclared(param.name.clone()));
            }
            self.declare_variable(
                param.name.clone(),
                param.ty.clone(),
                param.visibility.clone(),
            );
        }

        // Check function body
        let has_return = self.check_block(&func_def.body)?;

        // Check return statement exists if return type is not void-like
        // Note: We don't have a void type, so we'll require all functions to return
        // For now, we'll check that return statements match the return type
        // The has_return check can be used for future void type support

        self.exit_scope();
        self.current_return_type = None;

        Ok(())
    }

    fn check_block(&mut self, block: &Block) -> TypeCheckResult<bool> {
        self.enter_scope();
        let mut has_return = false;

        for stmt in &block.statements {
            match stmt {
                Statement::Return(_) => {
                    has_return = true;
                    self.check_statement(stmt)?;
                }
                _ => {
                    self.check_statement(stmt)?;
                }
            }
        }

        self.exit_scope();
        Ok(has_return)
    }

    fn check_statement(&mut self, stmt: &Statement) -> TypeCheckResult<()> {
        match stmt {
            Statement::VariableDecl(decl) => self.check_variable_decl(decl),
            Statement::Assignment(assign) => self.check_assignment(assign),
            Statement::If(if_stmt) => self.check_if_statement(if_stmt),
            Statement::ForLoop(for_loop) => self.check_for_loop(for_loop),
            Statement::Return(ret) => self.check_return_statement(ret),
            Statement::Directive(dir) => self.check_directive(dir),
            Statement::Expr(expr) => {
                self.check_expression(expr)?;
                Ok(())
            }
        }
    }

    fn check_variable_decl(&mut self, decl: &VariableDecl) -> TypeCheckResult<()> {
        self.check_type_expr(&decl.ty)?;

        if self.lookup_variable(&decl.name).is_some() {
            return Err(TypeError::VariableAlreadyDeclared(decl.name.clone()));
        }

        if let Some(init) = &decl.initializer {
            let init_type = self.check_expression(init)?;
            if !self.types_compatible(&decl.ty, &init_type) {
                return Err(TypeError::TypeMismatch {
                    expected: self.type_to_string(&decl.ty),
                    found: self.type_to_string(&init_type),
                });
            }
        }

        self.declare_variable(decl.name.clone(), decl.ty.clone(), decl.visibility.clone());
        Ok(())
    }

    fn check_assignment(&mut self, assign: &Assignment) -> TypeCheckResult<()> {
        let lvalue_type = self.check_lvalue(&assign.lvalue)?;
        let value_type = self.check_expression(&assign.value)?;

        if !self.types_compatible(&lvalue_type, &value_type) {
            return Err(TypeError::TypeMismatch {
                expected: self.type_to_string(&lvalue_type),
                found: self.type_to_string(&value_type),
            });
        }

        Ok(())
    }

    fn check_if_statement(&mut self, if_stmt: &IfStatement) -> TypeCheckResult<()> {
        let cond_type = self.check_expression(&if_stmt.condition)?;
        if !self.is_bool_type(&cond_type) {
            return Err(TypeError::TypeMismatch {
                expected: "Bool".to_string(),
                found: self.type_to_string(&cond_type),
            });
        }

        self.check_block(&if_stmt.then_block)?;
        if let Some(else_block) = &if_stmt.else_block {
            self.check_block(else_block)?;
        }

        Ok(())
    }

    fn check_for_loop(&mut self, for_loop: &ForLoop) -> TypeCheckResult<()> {
        self.enter_scope();
        // For loop variable is implicitly an integer (index)
        // We don't have an integer type in the AST, but the loop variable is used as an index
        // For now, we'll just check the body
        self.check_block(&for_loop.body)?;
        self.exit_scope();
        Ok(())
    }

    fn check_return_statement(&mut self, ret: &ReturnStatement) -> TypeCheckResult<()> {
        let return_type = self.check_expression(&ret.value)?;
        
        if let Some(expected_type) = &self.current_return_type {
            if !self.types_compatible(expected_type, &return_type) {
                return Err(TypeError::ReturnTypeMismatch {
                    expected: self.type_to_string(expected_type),
                    found: self.type_to_string(&return_type),
                });
            }
        } else {
            return Err(TypeError::ReturnTypeMismatch {
                expected: "none".to_string(),
                found: self.type_to_string(&return_type),
            });
        }

        Ok(())
    }

    fn check_directive(&mut self, dir: &Directive) -> TypeCheckResult<()> {
        match dir {
            Directive::Print(print) => {
                for expr in &print.expressions {
                    self.check_expression(expr)?;
                }
                Ok(())
            }
            Directive::Assert(assert) => {
                let cond_type = self.check_expression(&assert.condition)?;
                if !self.is_bool_type(&cond_type) {
                    return Err(TypeError::TypeMismatch {
                        expected: "Bool".to_string(),
                        found: self.type_to_string(&cond_type),
                    });
                }
                Ok(())
            }
            Directive::Abort(_) => Ok(()),
            Directive::Reveal(reveal) => {
                // Check that the identifier exists
                self.lookup_variable(&reveal.identifier)
                    .ok_or_else(|| TypeError::UndefinedVariable(reveal.identifier.clone()))?;
                Ok(())
            }
            Directive::Debug(debug) => {
                self.check_block(&debug.block)?;
                Ok(())
            }
        }
    }

    fn check_expression(&self, expr: &Expression) -> TypeCheckResult<TypeExpr> {
        let mut base_type = match &expr.base {
            ExpressionBase::Literal(lit) => self.check_literal(lit)?,
            ExpressionBase::ArrayLiteral(arr) => self.check_array_literal(arr)?,
            ExpressionBase::StructLiteral(struct_lit) => self.check_struct_literal(struct_lit)?,
            ExpressionBase::FunctionCall(call) => self.check_function_call(call)?,
            ExpressionBase::Identifier(name) => {
                let var_info = self.lookup_variable(name)
                    .ok_or_else(|| TypeError::UndefinedVariable(name.clone()))?;
                var_info.ty.clone()
            }
            ExpressionBase::BinaryOp(bin_op) => self.check_binary_op(bin_op)?,
            ExpressionBase::UnaryOp(unary_op) => self.check_unary_op(unary_op)?,
            ExpressionBase::Parenthesized(expr) => self.check_expression(expr)?,
        };

        // Apply postfix accesses
        for access in &expr.accesses {
            base_type = self.apply_access(&base_type, access)?;
        }

        Ok(base_type)
    }

    fn check_literal(&self, lit: &Literal) -> TypeCheckResult<TypeExpr> {
        match lit {
            Literal::Integer(_) => {
                // Integers are represented as Field<size> in MPC
                // For now, we'll use Field<64> as default for integer literals
                // This might need adjustment based on IR requirements
                Ok(TypeExpr::Base(BaseType::Field(FieldType { size: 64 })))
            }
            Literal::Bool(_) => Ok(TypeExpr::Base(BaseType::Bool)),
        }
    }

    fn check_array_literal(&self, arr: &ArrayLiteral) -> TypeCheckResult<TypeExpr> {
        if arr.elements.is_empty() {
            return Err(TypeError::TypeMismatch {
                expected: "non-empty array".to_string(),
                found: "empty array".to_string(),
            });
        }

        let first_type = self.check_expression(&arr.elements[0])?;
        
        for (i, elem) in arr.elements.iter().enumerate().skip(1) {
            let elem_type = self.check_expression(elem)?;
            if !self.types_compatible(&first_type, &elem_type) {
                return Err(TypeError::ArrayElementTypeMismatch {
                    expected: self.type_to_string(&first_type),
                    found: self.type_to_string(&elem_type),
                });
            }
        }

        Ok(TypeExpr::Array(ArrayType {
            element_type: Box::new(first_type),
            size: arr.elements.len() as u64,
        }))
    }

    fn check_struct_literal(&self, struct_lit: &StructLiteral) -> TypeCheckResult<TypeExpr> {
        let struct_def = self.structs.get(&struct_lit.struct_name)
            .ok_or_else(|| TypeError::UndefinedStruct(struct_lit.struct_name.clone()))?;

        // Build a map of provided fields for quick lookup
        let mut provided_fields: std::collections::HashMap<String, &FieldInit> = struct_lit.fields.iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        // Check for duplicate fields
        if provided_fields.len() != struct_lit.fields.len() {
            // Find the duplicate
            let mut seen = std::collections::HashSet::new();
            for field_init in &struct_lit.fields {
                if !seen.insert(&field_init.name) {
                    return Err(TypeError::ExtraStructField {
                        struct_name: struct_lit.struct_name.clone(),
                        field_name: field_init.name.clone(),
                    });
                }
            }
        }

        // Check all required fields are present and types match
        for field_def in &struct_def.fields {
            let field_init = provided_fields.get(&field_def.name)
                .ok_or_else(|| TypeError::MissingStructField {
                    struct_name: struct_lit.struct_name.clone(),
                    field_name: field_def.name.clone(),
                })?;

            let init_type = self.check_expression(&field_init.value)?;
            if !self.types_compatible(&field_def.ty, &init_type) {
                return Err(TypeError::TypeMismatch {
                    expected: self.type_to_string(&field_def.ty),
                    found: self.type_to_string(&init_type),
                });
            }
        }

        // Check no extra fields
        for field_init in &struct_lit.fields {
            if !struct_def.fields.iter().any(|f| f.name == field_init.name) {
                return Err(TypeError::ExtraStructField {
                    struct_name: struct_lit.struct_name.clone(),
                    field_name: field_init.name.clone(),
                });
            }
        }

        Ok(TypeExpr::Struct(StructType {
            name: struct_lit.struct_name.clone(),
        }))
    }

    fn check_function_call(&self, call: &FunctionCall) -> TypeCheckResult<TypeExpr> {
        let signature = self.functions.get(&call.name)
            .ok_or_else(|| TypeError::UndefinedFunction(call.name.clone()))?;

        if call.arguments.len() != signature.params.len() {
            return Err(TypeError::ArgumentCountMismatch {
                expected: signature.params.len(),
                found: call.arguments.len(),
            });
        }

        for (i, (arg, (param_ty, _))) in call.arguments.iter().zip(signature.params.iter()).enumerate() {
            let arg_type = self.check_expression(arg)?;
            if !self.types_compatible(param_ty, &arg_type) {
                return Err(TypeError::ArgumentTypeMismatch {
                    position: i,
                    expected: self.type_to_string(param_ty),
                    found: self.type_to_string(&arg_type),
                });
            }
        }

        Ok(signature.return_type.clone())
    }

    fn check_binary_op(&self, bin_op: &BinaryOp) -> TypeCheckResult<TypeExpr> {
        match bin_op {
            BinaryOp::LogicalOr(left, right) | BinaryOp::LogicalAnd(left, right) => {
                let left_type = self.check_expression(left)?;
                let right_type = self.check_expression(right)?;
                
                if !self.is_bool_type(&left_type) || !self.is_bool_type(&right_type) {
                    return Err(TypeError::InvalidOperator {
                        op: format!("{:?}", bin_op),
                        ty: format!("{:?}, {:?}", left_type, right_type),
                    });
                }
                Ok(TypeExpr::Base(BaseType::Bool))
            }
            BinaryOp::Comparison(_, left, right) => {
                let left_type = self.check_expression(left)?;
                let right_type = self.check_expression(right)?;
                
                if !self.types_compatible(&left_type, &right_type) {
                    return Err(TypeError::TypeMismatch {
                        expected: self.type_to_string(&left_type),
                        found: self.type_to_string(&right_type),
                    });
                }
                Ok(TypeExpr::Base(BaseType::Bool))
            }
            BinaryOp::BitwiseOr(left, right)
            | BinaryOp::BitwiseXor(left, right)
            | BinaryOp::BitwiseAnd(left, right)
            | BinaryOp::Shift(_, left, right)
            | BinaryOp::Add(left, right)
            | BinaryOp::Subtract(left, right)
            | BinaryOp::Multiply(left, right)
            | BinaryOp::Divide(left, right)
            | BinaryOp::Modulo(left, right) => {
                let left_type = self.check_expression(left)?;
                let right_type = self.check_expression(right)?;
                
                if !self.types_compatible(&left_type, &right_type) {
                    return Err(TypeError::TypeMismatch {
                        expected: self.type_to_string(&left_type),
                        found: self.type_to_string(&right_type),
                    });
                }
                
                // These operations work on Field types
                if !self.is_field_type(&left_type) {
                    return Err(TypeError::InvalidOperator {
                        op: format!("{:?}", bin_op),
                        ty: self.type_to_string(&left_type),
                    });
                }
                
                Ok(left_type)
            }
        }
    }

    fn check_unary_op(&self, unary_op: &UnaryOp) -> TypeCheckResult<TypeExpr> {
        match unary_op {
            UnaryOp::Not(expr) => {
                let expr_type = self.check_expression(expr)?;
                if !self.is_bool_type(&expr_type) {
                    return Err(TypeError::InvalidUnaryOperator {
                        op: "!".to_string(),
                        ty: self.type_to_string(&expr_type),
                    });
                }
                Ok(TypeExpr::Base(BaseType::Bool))
            }
            UnaryOp::Negate(expr) | UnaryOp::BitwiseNot(expr) => {
                let expr_type = self.check_expression(expr)?;
                if !self.is_field_type(&expr_type) {
                    return Err(TypeError::InvalidUnaryOperator {
                        op: format!("{:?}", unary_op),
                        ty: self.type_to_string(&expr_type),
                    });
                }
                Ok(expr_type)
            }
        }
    }

    fn check_lvalue(&self, lvalue: &LValue) -> TypeCheckResult<TypeExpr> {
        let var_info = self.lookup_variable(&lvalue.base)
            .ok_or_else(|| TypeError::UndefinedVariable(lvalue.base.clone()))?;
        
        let mut ty = var_info.ty.clone();
        
        for access in &lvalue.accesses {
            ty = self.apply_lvalue_access(&ty, access)?;
        }
        
        Ok(ty)
    }

    fn apply_access(&self, ty: &TypeExpr, access: &ExprAccess) -> TypeCheckResult<TypeExpr> {
        match access {
            ExprAccess::Array(index_expr) => {
                let index_type = self.check_expression(index_expr)?;
                if !self.is_field_type(&index_type) {
                    return Err(TypeError::InvalidArrayIndex(self.type_to_string(&index_type)));
                }
                
                match ty {
                    TypeExpr::Array(arr_ty) => Ok(*arr_ty.element_type.clone()),
                    _ => Err(TypeError::InvalidArrayAccess(self.type_to_string(ty))),
                }
            }
            ExprAccess::Field(field_name) => {
                match ty {
                    TypeExpr::Struct(struct_ty) => {
                        let struct_def = self.structs.get(&struct_ty.name)
                            .ok_or_else(|| TypeError::UndefinedStruct(struct_ty.name.clone()))?;
                        
                        let field = struct_def.fields.iter()
                            .find(|f| f.name == *field_name)
                            .ok_or_else(|| TypeError::InvalidFieldAccess {
                                struct_name: struct_ty.name.clone(),
                                field_name: field_name.clone(),
                            })?;
                        
                        Ok(field.ty.clone())
                    }
                    _ => Err(TypeError::InvalidFieldAccess {
                        struct_name: self.type_to_string(ty),
                        field_name: field_name.clone(),
                    }),
                }
            }
        }
    }

    fn apply_lvalue_access(&self, ty: &TypeExpr, access: &LValueAccess) -> TypeCheckResult<TypeExpr> {
        match access {
            LValueAccess::Array(index_expr) => {
                let index_type = self.check_expression(index_expr)?;
                if !self.is_field_type(&index_type) {
                    return Err(TypeError::InvalidArrayIndex(self.type_to_string(&index_type)));
                }
                
                match ty {
                    TypeExpr::Array(arr_ty) => Ok(*arr_ty.element_type.clone()),
                    _ => Err(TypeError::InvalidArrayAccess(self.type_to_string(ty))),
                }
            }
            LValueAccess::Field(field_name) => {
                match ty {
                    TypeExpr::Struct(struct_ty) => {
                        let struct_def = self.structs.get(&struct_ty.name)
                            .ok_or_else(|| TypeError::UndefinedStruct(struct_ty.name.clone()))?;
                        
                        let field = struct_def.fields.iter()
                            .find(|f| f.name == *field_name)
                            .ok_or_else(|| TypeError::InvalidFieldAccess {
                                struct_name: struct_ty.name.clone(),
                                field_name: field_name.clone(),
                            })?;
                        
                        Ok(field.ty.clone())
                    }
                    _ => Err(TypeError::InvalidFieldAccess {
                        struct_name: self.type_to_string(ty),
                        field_name: field_name.clone(),
                    }),
                }
            }
        }
    }

    fn check_type_expr(&self, ty: &TypeExpr) -> TypeCheckResult<()> {
        match ty {
            TypeExpr::Base(base) => match base {
                BaseType::Field(_) | BaseType::Bool => Ok(()),
            },
            TypeExpr::Array(arr_ty) => {
                self.check_type_expr(&arr_ty.element_type)?;
                Ok(())
            }
            TypeExpr::Struct(struct_ty) => {
                self.structs.get(&struct_ty.name)
                    .ok_or_else(|| TypeError::UndefinedStruct(struct_ty.name.clone()))?;
                Ok(())
            }
        }
    }

    fn types_compatible(&self, expected: &TypeExpr, found: &TypeExpr) -> bool {
        match (expected, found) {
            (TypeExpr::Base(BaseType::Bool), TypeExpr::Base(BaseType::Bool)) => true,
            (TypeExpr::Base(BaseType::Field(f1)), TypeExpr::Base(BaseType::Field(f2))) => {
                // Field types with different sizes might be compatible depending on IR rules
                // For now, we'll require exact match
                f1.size == f2.size
            }
            (TypeExpr::Array(a1), TypeExpr::Array(a2)) => {
                self.types_compatible(&a1.element_type, &a2.element_type) && a1.size == a2.size
            }
            (TypeExpr::Struct(s1), TypeExpr::Struct(s2)) => s1.name == s2.name,
            _ => false,
        }
    }

    fn is_bool_type(&self, ty: &TypeExpr) -> bool {
        matches!(ty, TypeExpr::Base(BaseType::Bool))
    }

    fn is_field_type(&self, ty: &TypeExpr) -> bool {
        matches!(ty, TypeExpr::Base(BaseType::Field(_)))
    }

    fn type_to_string(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Base(BaseType::Bool) => "Bool".to_string(),
            TypeExpr::Base(BaseType::Field(f)) => format!("Field<{}>", f.size),
            TypeExpr::Array(arr) => format!("Array<{}, {}>", self.type_to_string(&arr.element_type), arr.size),
            TypeExpr::Struct(s) => s.name.clone(),
        }
    }

    // Scope management
    fn enter_scope(&mut self) {
        self.variables.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.variables.pop();
    }

    fn declare_variable(&mut self, name: String, ty: TypeExpr, visibility: Visibility) {
        if let Some(scope) = self.variables.last_mut() {
            scope.insert(name, VariableInfo { ty, visibility });
        }
    }

    fn lookup_variable(&self, name: &str) -> Option<&VariableInfo> {
        for scope in self.variables.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }
}

pub fn type_check(program: &Program) -> TypeCheckResult<()> {
    let mut checker = TypeChecker::new();
    checker.check_program(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::*;

    fn parse_and_type_check(source: &str) -> Result<Program, TypeError> {
        let pairs = parse_program(source).unwrap();
        let program = build_ast(pairs).unwrap();
        type_check(&program)?;
        Ok(program)
    }

    #[test]
    fn test_type_check_valid_function() {
        let source = "fn add(Public Field<64> a, Public Field<64> b) -> Field<64> { return a + b; }";
        let result = parse_and_type_check(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_type_check_type_mismatch() {
        let source = "fn test(Public Field<64> a) -> Bool { return a; }";
        let result = parse_and_type_check(source);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TypeError::ReturnTypeMismatch { .. }));
    }

    #[test]
    fn test_type_check_undefined_variable() {
        let source = "fn test() -> Field<64> { return x; }";
        let result = parse_and_type_check(source);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TypeError::UndefinedVariable(_)));
    }

    #[test]
    fn test_type_check_valid_if() {
        let source = "fn test(Public Field<64> a) -> Field<64> { if a > 0 { return 1; } else { return 0; } }";
        let result = parse_and_type_check(source);
        assert!(result.is_ok());
    }
}

