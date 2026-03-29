From Stdlib Require Import Strings.String.
From Stdlib Require Import Lists.List.
Import ListNotations.

From HelixDSL Require Import Types.
From HelixDSL Require Import Values.

(** * Helix DSL — Abstract Syntax

    This file defines the abstract syntax of the Helix MPC DSL: binary
    and unary operators, expressions, and statements.  The AST
    corresponds to the source level before any compilation pass.

    Key design points that differ from a general-purpose language:
    - Array indices must be compile-time constants ([EIndex] takes [nat]).
    - For loops are fully unrolled at codegen; the bounds are [nat].
    - All function calls are eliminated by inlining; [ECall] appears
      in the source AST but never in an evaluated circuit.
    - If-else is compiled to a multiplexer ([ESelect]) so that both
      branches are always computed — mandatory for MPC protocols. *)

(* ------------------------------------------------------------------ *)
(** ** Operators *)

Inductive binop : Type :=
  (* Arithmetic on field elements *)
  | OpAdd | OpSub | OpMul | OpDiv | OpMod
  (* Bitwise operations on field elements *)
  | OpBAnd | OpBOr | OpBXor | OpShl | OpShr
  (* Logical operations on booleans (eager in circuits) *)
  | OpLAnd | OpLOr
  (* Comparisons — always produce Bool *)
  | OpEq | OpNeq | OpLt | OpLe | OpGt | OpGe.

Inductive unop : Type :=
  | OpNeg    (** Arithmetic negation:  [- e]   on field elements *)
  | OpNot    (** Boolean negation:     [! e]   on booleans *)
  | OpBNot.  (** Bitwise complement:   [~ e]   on field elements *)

(* ------------------------------------------------------------------ *)
(** ** Expressions *)

Inductive expr : Type :=
  | EVar    : string -> expr
    (** Variable reference. *)

  | EConst  : value -> expr
    (** Compile-time constant (field element, bool, or aggregate). *)

  | EBinop  : binop -> expr -> expr -> expr
    (** Binary operation. *)

  | EUnop   : unop -> expr -> expr
    (** Unary operation. *)

  | ESelect : expr -> expr -> expr -> expr
    (** Multiplexer: [ESelect cond e_then e_else].
        Both branches are always evaluated; the result is selected by
        [cond].  This primitive arises from compiling [if]-expressions
        and is the fundamental conditional in the circuit model. *)

  | EIndex  : expr -> nat -> expr
    (** Array access with a compile-time constant index.
        [EIndex arr i] reads element [i] from array [arr]. *)

  | EField  : expr -> string -> expr
    (** Struct field access: [EField e "fname"]. *)

  | ECall   : string -> list expr -> expr.
    (** Function call [f(args)].
        All calls are inlined during codegen; this constructor is
        present in source ASTs but eliminated before evaluation. *)

(* ------------------------------------------------------------------ *)
(** ** Statements *)

Inductive stmt : Type :=
  | SLet    : visibility -> ty -> string -> expr -> stmt
    (** [SLet vis τ x e]: introduce a new binding [let vis τ x = e]. *)

  | SAssign : string -> expr -> stmt
    (** [SAssign x e]: overwrite an existing variable [x = e]. *)

  | SIf     : expr -> list stmt -> list stmt -> stmt
    (** [SIf cond s_then s_else]: conditional branch.
        Both branches are type-checked independently; at the circuit
        level this desugars into [ESelect] gates. *)

  | SReturn : expr -> stmt
    (** [SReturn e]: return [e] from the enclosing function. *)

  | SFor    : string -> nat -> nat -> list stmt -> stmt.
    (** [SFor x lo hi body]: iterate [x] over [lo .. hi).
        The loop is fully unrolled at codegen; [lo] and [hi] must be
        statically known. *)

(* ------------------------------------------------------------------ *)
(** ** Function definitions and programs *)

(** A single function parameter: visibility, type, name. *)
Record param : Type := MkParam {
  param_vis  : visibility;
  param_type : ty;
  param_name : string;
}.

(** A top-level function definition. *)
Record func_def : Type := MkFuncDef {
  fd_name    : string;
  fd_params  : list param;
  fd_ret_ty  : ty;
  fd_body    : list stmt;
}.

(** A complete program is a list of function definitions.
    The entry point is the function named ["main"]. *)
Record program : Type := MkProgram {
  prog_funcs : list func_def;
}.

(* ------------------------------------------------------------------ *)
(** ** Structural predicates *)

(** [no_calls e]: [e] contains no [ECall] sub-expressions.
    The evaluation semantics is defined for call-free expressions;
    calls require prior inlining. *)
Inductive no_calls : expr -> Prop :=
  | NcVar    : forall x,       no_calls (EVar x)
  | NcConst  : forall v,       no_calls (EConst v)
  | NcBinop  : forall op e1 e2,
      no_calls e1 -> no_calls e2 -> no_calls (EBinop op e1 e2)
  | NcUnop   : forall op e,
      no_calls e  -> no_calls (EUnop op e)
  | NcSelect : forall e1 e2 e3,
      no_calls e1 -> no_calls e2 -> no_calls e3 ->
      no_calls (ESelect e1 e2 e3)
  | NcIndex  : forall e i,
      no_calls e  -> no_calls (EIndex e i)
  | NcField  : forall e f,
      no_calls e  -> no_calls (EField e f).
  (* ECall is deliberately absent. *)

(** [no_div e]: [e] contains no division or modulo operations.
    Used to state a total-progress theorem (division by zero is stuck). *)
Inductive no_div : expr -> Prop :=
  | NdVar    : forall x,       no_div (EVar x)
  | NdConst  : forall v,       no_div (EConst v)
  | NdBinop  : forall op e1 e2,
      op <> OpDiv -> op <> OpMod ->
      no_div e1 -> no_div e2 ->
      no_div (EBinop op e1 e2)
  | NdUnop   : forall op e,    no_div e  -> no_div (EUnop op e)
  | NdSelect : forall e1 e2 e3,
      no_div e1 -> no_div e2 -> no_div e3 ->
      no_div (ESelect e1 e2 e3)
  | NdIndex  : forall e i,     no_div e  -> no_div (EIndex e i)
  | NdField  : forall e f,     no_div e  -> no_div (EField e f)
  | NdCall   : forall f args,
      Forall no_div args -> no_div (ECall f args).
