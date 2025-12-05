// Abstract Syntax Tree for MPC DSL

use serde::{Deserialize, Serialize};

// ============================================================================
// Program Structure
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Item {
    StructDef(StructDef),
    FunctionDef(FunctionDef),
}

// ============================================================================
// Struct Definitions
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructField {
    pub ty: TypeExpr,
    pub name: String,
}

// ============================================================================
// Function Definitions
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub visibility: Visibility,
    pub ty: TypeExpr,
    pub name: String,
}

// ============================================================================
// Type System
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeExpr {
    Base(BaseType),
    Array(ArrayType),
    Struct(StructType),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BaseType {
    Field(FieldType),
    Bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldType {
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrayType {
    pub element_type: Box<TypeExpr>,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructType {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Secret,
}

// ============================================================================
// Statements
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    VariableDecl(VariableDecl),
    Assignment(Assignment),
    If(IfStatement),
    ForLoop(ForLoop),
    Return(ReturnStatement),
    Directive(Directive),
    Expr(Expression),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableDecl {
    pub visibility: Visibility,
    pub ty: TypeExpr,
    pub name: String,
    pub initializer: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub lvalue: LValue,
    pub value: Expression,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LValue {
    pub base: String,
    pub accesses: Vec<LValueAccess>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LValueAccess {
    Array(Expression),
    Field(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_block: Block,
    pub else_block: Option<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForLoop {
    pub var_name: String,
    pub start: u64,
    pub end: u64,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnStatement {
    pub value: Expression,
}

// ============================================================================
// Directives
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Directive {
    Print(PrintDirective),
    Assert(AssertDirective),
    Abort(AbortDirective),
    Reveal(RevealDirective),
    Debug(DebugDirective),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintDirective {
    pub expressions: Vec<Expression>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertDirective {
    pub condition: Expression,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbortDirective {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevealDirective {
    pub identifier: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebugDirective {
    pub block: Block,
}

// ============================================================================
// Expressions
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expression {
    pub base: ExpressionBase,
    pub accesses: Vec<ExprAccess>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExpressionBase {
    Literal(Literal),
    ArrayLiteral(ArrayLiteral),
    StructLiteral(StructLiteral),
    FunctionCall(FunctionCall),
    Identifier(String),
    BinaryOp(BinaryOp),
    UnaryOp(UnaryOp),
    Parenthesized(Box<Expression>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinaryOp {
    LogicalOr(Box<Expression>, Box<Expression>),
    LogicalAnd(Box<Expression>, Box<Expression>),
    Comparison(ComparisonOp, Box<Expression>, Box<Expression>),
    BitwiseOr(Box<Expression>, Box<Expression>),
    BitwiseXor(Box<Expression>, Box<Expression>),
    BitwiseAnd(Box<Expression>, Box<Expression>),
    Shift(ShiftOp, Box<Expression>, Box<Expression>),
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Divide(Box<Expression>, Box<Expression>),
    Modulo(Box<Expression>, Box<Expression>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComparisonOp {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShiftOp {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnaryOp {
    Negate(Box<Expression>),
    Not(Box<Expression>),
    BitwiseNot(Box<Expression>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Integer(u64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrayLiteral {
    pub elements: Vec<Expression>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructLiteral {
    pub struct_name: String,
    pub fields: Vec<FieldInit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldInit {
    pub name: String,
    pub value: Expression,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: Vec<Expression>,
}

// ============================================================================
// Expression Access (for postfix operations)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExprAccess {
    Array(Box<Expression>),
    Field(String),
}

