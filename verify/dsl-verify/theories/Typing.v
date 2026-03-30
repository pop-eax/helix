From Stdlib Require Import Strings.String.
From Stdlib Require Import Lists.List.
Import ListNotations.

From HelixDSL Require Import Types.
From HelixDSL Require Import Values.
From HelixDSL Require Import Syntax.

(** * Helix DSL — Type System (Judgments)

    The type system is syntax-directed: each expression constructor
    has exactly one applicable rule, so type inference is decidable.

    Environments:
      [Γ : ty_env]      — variable name → type
      [Φ : func_ty_env] — function name → (param types, return type)
      [Σ : struct_env]  — struct name → (field name, field type) list

    The main judgments are:
      [has_type_expr Γ Φ e τ]           — expression typing
      [has_type_stmt  Γ Φ Σ τ_ret s  Γ'] — statement typing
      [has_type_stmts Γ Φ Σ τ_ret ss Γ'] — statement list typing *)

(* ------------------------------------------------------------------ *)
(** ** Environments *)

(** Variable environment: most-recently-added binding shadows earlier ones. *)
Definition ty_env := list (string * ty).

Fixpoint ty_lookup (Γ : ty_env) (x : string) : option ty :=
  match Γ with
  | []            => None
  | (y, τ) :: Γ' => if String.eqb x y then Some τ else ty_lookup Γ' x
  end.

(** Function type environment. *)
Definition func_ty_env := list (string * (list ty * ty)).

Fixpoint func_ty_lookup (Φ : func_ty_env) (f : string) : option (list ty * ty) :=
  match Φ with
  | []              => None
  | (g, sig) :: Φ' => if String.eqb f g then Some sig else func_ty_lookup Φ' f
  end.

(** Struct definition environment: maps a struct name to its ordered
    list of (field_name, field_type) pairs. *)
Definition struct_env := list (string * list (string * ty)).

Fixpoint struct_lookup (Σ : struct_env) (name : string)
    : option (list (string * ty)) :=
  match Σ with
  | []              => None
  | (s, fs) :: Σ' => if String.eqb name s then Some fs else struct_lookup Σ' s
  end.

Fixpoint field_lookup (fields : list (string * ty)) (fname : string)
    : option ty :=
  match fields with
  | []              => None
  | (f, τ) :: rest => if String.eqb fname f then Some τ else field_lookup rest fname
  end.

(* ------------------------------------------------------------------ *)
(** ** Operator result types *)

(** [binop_type op τ_operand] returns the result type of [op] when both
    operands have type [τ_operand], or [None] if the combination is
    ill-typed. *)
Definition binop_type (op : binop) (τ : ty) : option ty :=
  match op, τ with
  (* Arithmetic: Field × Field → same Field *)
  | OpAdd, TBase (BTField n) => Some (TField n)
  | OpSub, TBase (BTField n) => Some (TField n)
  | OpMul, TBase (BTField n) => Some (TField n)
  | OpDiv, TBase (BTField n) => Some (TField n)
  | OpMod, TBase (BTField n) => Some (TField n)
  (* Bitwise: Field × Field → same Field *)
  | OpBAnd, TBase (BTField n) => Some (TField n)
  | OpBOr,  TBase (BTField n) => Some (TField n)
  | OpBXor, TBase (BTField n) => Some (TField n)
  | OpShl,  TBase (BTField n) => Some (TField n)
  | OpShr,  TBase (BTField n) => Some (TField n)
  (* Logical: Bool × Bool → Bool *)
  | OpLAnd, TBase BTBool => Some TBoolTy
  | OpLOr,  TBase BTBool => Some TBoolTy
  (* Comparison: any base type × same base type → Bool *)
  | OpEq,  TBase _ => Some TBoolTy
  | OpNeq, TBase _ => Some TBoolTy
  | OpLt,  TBase _ => Some TBoolTy
  | OpLe,  TBase _ => Some TBoolTy
  | OpGt,  TBase _ => Some TBoolTy
  | OpGe,  TBase _ => Some TBoolTy
  (* Everything else is ill-typed *)
  | _, _ => None
  end.

Definition unop_type (op : unop) (τ : ty) : option ty :=
  match op, τ with
  | OpNeg,  TBase (BTField n) => Some (TField n)
  | OpNot,  TBase BTBool      => Some TBoolTy
  | OpBNot, TBase (BTField n) => Some (TField n)
  | _, _                      => None
  end.

(* ------------------------------------------------------------------ *)
(** ** Typing judgments *)

(** [has_type_expr Γ Φ Σ e τ]: expression [e] has type [τ] in
    environments [Γ] (variables), [Φ] (functions), [Σ] (structs). *)
Inductive has_type_expr
    (Γ : ty_env) (Φ : func_ty_env) (Σ : struct_env)
    : expr -> ty -> Prop :=

  | TyVar : forall x τ,
      ty_lookup Γ x = Some τ ->
      has_type_expr Γ Φ Σ (EVar x) τ

  | TyConst : forall v τ,
      wf_value v τ ->
      has_type_expr Γ Φ Σ (EConst v) τ

  | TyBinop : forall op e1 e2 τ τ_res,
      has_type_expr Γ Φ Σ e1 τ ->
      has_type_expr Γ Φ Σ e2 τ ->
      binop_type op τ = Some τ_res ->
      has_type_expr Γ Φ Σ (EBinop op e1 e2) τ_res

  | TyUnop : forall op e τ τ_res,
      has_type_expr Γ Φ Σ e τ ->
      unop_type op τ = Some τ_res ->
      has_type_expr Γ Φ Σ (EUnop op e) τ_res

  | TySelect : forall e_cond e_then e_else τ,
      has_type_expr Γ Φ Σ e_cond TBoolTy ->
      has_type_expr Γ Φ Σ e_then τ ->
      has_type_expr Γ Φ Σ e_else τ ->
      has_type_expr Γ Φ Σ (ESelect e_cond e_then e_else) τ

  | TyIndex : forall e_arr i τ n,
      has_type_expr Γ Φ Σ e_arr (TArray τ n) ->
      i < n ->
      has_type_expr Γ Φ Σ (EIndex e_arr i) τ

  | TyField : forall e_struct sname fname τ_field field_defs,
      has_type_expr Γ Φ Σ e_struct (TStruct sname) ->
      struct_lookup Σ sname = Some field_defs ->
      field_lookup field_defs fname = Some τ_field ->
      has_type_expr Γ Φ Σ (EField e_struct fname) τ_field

  | TyCall : forall f args τ_params τ_ret,
      func_ty_lookup Φ f = Some (τ_params, τ_ret) ->
      List.length args = List.length τ_params ->
      Forall2 (fun e τ => has_type_expr Γ Φ Σ e τ) args τ_params ->
      has_type_expr Γ Φ Σ (ECall f args) τ_ret.

(** [has_type_stmt Γ Φ Σ τ_ret s Γ']: statement [s] is well-typed in
    [Γ] with expected return type [τ_ret], producing output environment
    [Γ']. *)
Inductive has_type_stmt
    (Φ : func_ty_env) (Σ : struct_env) (τ_ret : ty)
    : ty_env -> stmt -> ty_env -> Prop :=

  | TyLet : forall Γ vis τ x e,
      has_type_expr Γ Φ Σ e τ ->
      has_type_stmt Φ Σ τ_ret Γ (SLet vis τ x e) ((x, τ) :: Γ)

  | TyAssign : forall Γ x e τ,
      ty_lookup Γ x = Some τ ->
      has_type_expr Γ Φ Σ e τ ->
      has_type_stmt Φ Σ τ_ret Γ (SAssign x e) Γ

  | TyIf : forall Γ e_cond s_then s_else Γ' Γ'',
      has_type_expr Γ Φ Σ e_cond TBoolTy ->
      has_type_stmts Φ Σ τ_ret Γ s_then Γ' ->
      has_type_stmts Φ Σ τ_ret Γ s_else Γ'' ->
      has_type_stmt Φ Σ τ_ret Γ (SIf e_cond s_then s_else) Γ

  | TyReturn : forall Γ e,
      has_type_expr Γ Φ Σ e τ_ret ->
      has_type_stmt Φ Σ τ_ret Γ (SReturn e) Γ

  | TyFor : forall Γ x lo hi body Γ_body,
      (** The loop variable [x] has type [Field<64>] inside the body.
          We require the body to type-check and produce some [Γ_body];
          the loop does not introduce [x] into the outer environment. *)
      has_type_stmts Φ Σ τ_ret ((x, TField 64) :: Γ) body Γ_body ->
      has_type_stmt Φ Σ τ_ret Γ (SFor x lo hi body) Γ

(** [has_type_stmts Γ Φ Σ τ_ret ss Γ']: statement list [ss] is
    well-typed, threading the environment through each statement. *)
with has_type_stmts
    (Φ : func_ty_env) (Σ : struct_env) (τ_ret : ty)
    : ty_env -> list stmt -> ty_env -> Prop :=

  | TyNil : forall Γ,
      has_type_stmts Φ Σ τ_ret Γ [] Γ

  | TyCons : forall Γ Γ' Γ'' s rest,
      has_type_stmt  Φ Σ τ_ret Γ  s    Γ'  ->
      has_type_stmts Φ Σ τ_ret Γ' rest Γ'' ->
      has_type_stmts Φ Σ τ_ret Γ (s :: rest) Γ''.

(* ------------------------------------------------------------------ *)
(** ** Well-typed functions and programs *)

(** Build the initial typing environment from a parameter list. *)
Fixpoint params_to_ty_env (ps : list param) : ty_env :=
  match ps with
  | []     => []
  | p :: rest => (param_name p, param_type p) :: params_to_ty_env rest
  end.

(** A function definition is well-typed if its body checks out under
    the parameter environment with the declared return type. *)
Definition wf_func_def (Φ : func_ty_env) (Σ : struct_env) (fd : func_def) : Prop :=
  exists Γ_out,
    has_type_stmts Φ Σ (fd_ret_ty fd)
                   (params_to_ty_env (fd_params fd))
                   (fd_body fd)
                   Γ_out.

(** Extract a function type environment from a program. *)
Definition func_ty_env_of (p : program) : func_ty_env :=
  List.map (fun fd =>
    (fd_name fd,
     (List.map param_type (fd_params fd), fd_ret_ty fd)))
    (prog_funcs p).

(** A program is well-typed if every function is well-typed under the
    program's own function environment. *)
Definition wf_program (p : program) (Σ : struct_env) : Prop :=
  let Φ := func_ty_env_of p in
  Forall (wf_func_def Φ Σ) (prog_funcs p).
