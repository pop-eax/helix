From Stdlib Require Import ZArith.
From Stdlib Require Import Lists.List.
From Stdlib Require Import Strings.String.
From Stdlib Require Import Bool.
Import ListNotations.

From HelixDSL Require Import Types.
From HelixDSL Require Import Values.
From HelixDSL Require Import Syntax.

(** * Helix DSL — Big-Step Operational Semantics

    We give a natural-semantics (big-step) specification of the Helix
    DSL in terms of inductive relations.  Using relations rather than
    computable functions avoids termination obligations while still
    providing full reasoning power.

    The semantics is call-by-value and sequential.  Statements thread
    an environment [ρ : val_env] through evaluation; a [SReturn]
    terminates the enclosing function immediately.

    Top-level judgments:
      [eval_expr  Φ ρ e       v ]  — expression [e] evaluates to [v]
      [eval_stmt  Φ ρ s       r ]  — statement [s] produces result [r]
      [eval_stmts Φ ρ ss      r ]  — statement list produces result [r]
      [eval_for   Φ ρ x lo hi body r] — for loop produces result [r] *)

Open Scope Z_scope.

(* ------------------------------------------------------------------ *)
(** ** Runtime environments *)

(** A value environment maps variable names to their current values.
    New bindings shadow older ones (list-as-stack). *)
Definition val_env := list (string * value).

Fixpoint val_lookup (ρ : val_env) (x : string) : option value :=
  match ρ with
  | []            => None
  | (y, v) :: ρ' => if String.eqb x y then Some v else val_lookup ρ' x
  end.

(** Push a new binding (or shadow an existing one). *)
Definition val_update (ρ : val_env) (x : string) (v : value) : val_env :=
  (x, v) :: ρ.

(** Function value environment: stores the parameter names and body of
    each function (enough information for call-by-value inlining). *)
Record func_val : Type := MkFuncVal {
  fv_params : list string;
  fv_body   : list stmt;
}.

Definition func_val_env := list (string * func_val).

Fixpoint func_val_lookup (Φ : func_val_env) (f : string) : option func_val :=
  match Φ with
  | []              => None
  | (g, fv) :: Φ' => if String.eqb f g then Some fv else func_val_lookup Φ' f
  end.

(** Build a function value environment from a program. *)
Definition func_val_env_of (p : program) : func_val_env :=
  List.map (fun fd =>
    (fd_name fd,
     MkFuncVal (List.map param_name (fd_params fd)) (fd_body fd)))
    (prog_funcs p).

(* ------------------------------------------------------------------ *)
(** ** Statement evaluation result *)

(** Evaluating a statement either:
    - falls through, producing an updated environment; or
    - hits a [return] statement, terminating with a value. *)
Inductive eval_result : Type :=
  | Continue  : val_env -> eval_result
  | ReturnVal : value   -> eval_result.

(* ------------------------------------------------------------------ *)
(** ** Concrete operator evaluation (computable helpers) *)

(** These functions compute operator results on concrete values.
    They are used as side-conditions in the big-step rules. *)

Definition eval_binop (op : binop) (v1 v2 : value) : option value :=
  match op, v1, v2 with
  | OpAdd, VField n a, VField m b =>
      if Nat.eqb n m then Some (VField n ((a + b) mod field_mod n)) else None
  | OpSub, VField n a, VField m b =>
      if Nat.eqb n m
      then Some (VField n ((a - b + field_mod n) mod field_mod n))
      else None
  | OpMul, VField n a, VField m b =>
      if Nat.eqb n m then Some (VField n ((a * b) mod field_mod n)) else None
  | OpDiv, VField n a, VField m b =>
      if Nat.eqb n m
      then if Z.eqb b 0 then None else Some (VField n (a / b))
      else None
  | OpMod, VField n a, VField m b =>
      if Nat.eqb n m
      then if Z.eqb b 0 then None else Some (VField n (a mod b))
      else None
  | OpBAnd, VField n a, VField m b =>
      if Nat.eqb n m then Some (VField n (Z.land a b)) else None
  | OpBOr,  VField n a, VField m b =>
      if Nat.eqb n m then Some (VField n (Z.lor  a b)) else None
  | OpBXor, VField n a, VField m b =>
      if Nat.eqb n m then Some (VField n (Z.lxor a b)) else None
  | OpShl,  VField n a, VField m b =>
      if Nat.eqb n m
      then Some (VField n (Z.shiftl a b mod field_mod n))
      else None
  | OpShr,  VField n a, VField m b =>
      if Nat.eqb n m then Some (VField n (Z.shiftr a b)) else None
  | OpLAnd, VBool a, VBool b => Some (VBool (andb a b))
  | OpLOr,  VBool a, VBool b => Some (VBool (orb  a b))
  | OpEq,  VField n a, VField m b =>
      if Nat.eqb n m then Some (VBool (Z.eqb a b))         else None
  | OpNeq, VField n a, VField m b =>
      if Nat.eqb n m then Some (VBool (negb (Z.eqb a b)))  else None
  | OpLt,  VField n a, VField m b =>
      if Nat.eqb n m then Some (VBool (Z.ltb a b))         else None
  | OpLe,  VField n a, VField m b =>
      if Nat.eqb n m then Some (VBool (Z.leb a b))         else None
  | OpGt,  VField n a, VField m b =>
      if Nat.eqb n m then Some (VBool (Z.gtb a b))         else None
  | OpGe,  VField n a, VField m b =>
      if Nat.eqb n m then Some (VBool (Z.geb a b))         else None
  | OpEq,  VBool a, VBool b => Some (VBool (Bool.eqb a b))
  | OpNeq, VBool a, VBool b => Some (VBool (negb (Bool.eqb a b)))
  | _, _, _ => None
  end.

Definition eval_unop (op : unop) (v : value) : option value :=
  match op, v with
  | OpNeg,  VField n z =>
      Some (VField n ((- z + field_mod n) mod field_mod n))
  | OpNot,  VBool b    => Some (VBool (negb b))
  | OpBNot, VField n z => Some (VField n (Z.lnot z mod field_mod n))
  | _, _ => None
  end.

(* ------------------------------------------------------------------ *)
(** ** Big-step evaluation relations *)

(** Five mutually inductive relations cover expressions, expression
    lists, single statements, statement lists, and for-loop unrolling. *)

Inductive eval_expr (Φ : func_val_env)
    : val_env -> expr -> value -> Prop :=

  | EvalVar : forall ρ x v,
      val_lookup ρ x = Some v ->
      eval_expr Φ ρ (EVar x) v

  | EvalConst : forall ρ v,
      eval_expr Φ ρ (EConst v) v

  | EvalBinop : forall ρ op e1 e2 v1 v2 v,
      eval_expr Φ ρ e1 v1 ->
      eval_expr Φ ρ e2 v2 ->
      eval_binop op v1 v2 = Some v ->
      eval_expr Φ ρ (EBinop op e1 e2) v

  | EvalUnop : forall ρ op e v1 v,
      eval_expr Φ ρ e v1 ->
      eval_unop op v1 = Some v ->
      eval_expr Φ ρ (EUnop op e) v

  | EvalSelectTrue : forall ρ e_cond e_then e_else v,
      eval_expr Φ ρ e_cond (VBool true) ->
      eval_expr Φ ρ e_then v ->
      eval_expr Φ ρ (ESelect e_cond e_then e_else) v

  | EvalSelectFalse : forall ρ e_cond e_then e_else v,
      eval_expr Φ ρ e_cond (VBool false) ->
      eval_expr Φ ρ e_else v ->
      eval_expr Φ ρ (ESelect e_cond e_then e_else) v

  | EvalIndex : forall ρ e_arr (τ : ty) i (vs : list value) v,
      eval_expr Φ ρ e_arr (VArray τ vs) ->
      nth_error vs i = Some v ->
      eval_expr Φ ρ (EIndex e_arr i) v

  | EvalField : forall ρ e_struct (sname fname : string)
                         (fields : list (string * value)) v,
      eval_expr Φ ρ e_struct (VStruct sname fields) ->
      List.find (fun p => String.eqb (fst p) fname) fields = Some (fname, v) ->
      eval_expr Φ ρ (EField e_struct fname) v

  | EvalCall : forall ρ f args fv arg_vals v,
      func_val_lookup Φ f = Some fv ->
      List.length (fv_params fv) = List.length args ->
      eval_exprs Φ ρ args arg_vals ->
      eval_stmts Φ (List.combine (fv_params fv) arg_vals) (fv_body fv)
                 (ReturnVal v) ->
      eval_expr Φ ρ (ECall f args) v

with eval_exprs (Φ : func_val_env)
    : val_env -> list expr -> list value -> Prop :=

  | EvalExprsNil : forall ρ,
      eval_exprs Φ ρ [] []

  | EvalExprsCons : forall ρ e es v vs,
      eval_expr Φ ρ e v ->
      eval_exprs Φ ρ es vs ->
      eval_exprs Φ ρ (e :: es) (v :: vs)

with eval_stmt (Φ : func_val_env)
    : val_env -> stmt -> eval_result -> Prop :=

  | EvalLet : forall ρ vis τ x e v,
      eval_expr Φ ρ e v ->
      eval_stmt Φ ρ (SLet vis τ x e) (Continue (val_update ρ x v))

  | EvalAssign : forall ρ x e v,
      eval_expr Φ ρ e v ->
      eval_stmt Φ ρ (SAssign x e) (Continue (val_update ρ x v))

  | EvalIfTrue : forall ρ e_cond s_then s_else r,
      eval_expr Φ ρ e_cond (VBool true) ->
      eval_stmts Φ ρ s_then r ->
      eval_stmt Φ ρ (SIf e_cond s_then s_else) r

  | EvalIfFalse : forall ρ e_cond s_then s_else r,
      eval_expr Φ ρ e_cond (VBool false) ->
      eval_stmts Φ ρ s_else r ->
      eval_stmt Φ ρ (SIf e_cond s_then s_else) r

  | EvalReturn : forall ρ e v,
      eval_expr Φ ρ e v ->
      eval_stmt Φ ρ (SReturn e) (ReturnVal v)

  | EvalFor : forall ρ x lo hi body r,
      eval_for Φ ρ x lo hi body r ->
      eval_stmt Φ ρ (SFor x lo hi body) r

with eval_stmts (Φ : func_val_env)
    : val_env -> list stmt -> eval_result -> Prop :=

  | EvalNil : forall ρ,
      eval_stmts Φ ρ [] (Continue ρ)

  | EvalConsContinue : forall ρ ρ' s rest r,
      eval_stmt  Φ ρ  s    (Continue ρ') ->
      eval_stmts Φ ρ' rest r ->
      eval_stmts Φ ρ (s :: rest) r

  | EvalConsReturn : forall ρ s rest v,
      eval_stmt Φ ρ s (ReturnVal v) ->
      eval_stmts Φ ρ (s :: rest) (ReturnVal v)

with eval_for (Φ : func_val_env)
    : val_env -> string -> nat -> nat -> list stmt -> eval_result -> Prop :=

  | EvalForDone : forall ρ x lo hi body,
      (lo >= hi)%nat ->
      eval_for Φ ρ x lo hi body (Continue ρ)

  | EvalForStep : forall ρ ρ' x lo hi body r,
      (lo < hi)%nat ->
      eval_stmts Φ (val_update ρ x (VField 64%nat (to_field 64%nat (Z.of_nat lo)))) body
                 (Continue ρ') ->
      eval_for Φ ρ' x (S lo) hi body r ->
      eval_for Φ ρ  x lo     hi body r

  | EvalForReturn : forall ρ x lo hi body v,
      (lo < hi)%nat ->
      eval_stmts Φ (val_update ρ x (VField 64%nat (to_field 64%nat (Z.of_nat lo)))) body
                 (ReturnVal v) ->
      eval_for Φ ρ x lo hi body (ReturnVal v).

(* ------------------------------------------------------------------ *)
(** ** Scheme for mutual induction *)

Scheme eval_expr_mut_ind  := Induction for eval_expr  Sort Prop
  with eval_exprs_mut_ind := Induction for eval_exprs Sort Prop
  with eval_stmt_mut_ind  := Induction for eval_stmt  Sort Prop
  with eval_stmts_mut_ind := Induction for eval_stmts Sort Prop
  with eval_for_mut_ind   := Induction for eval_for   Sort Prop.

Combined Scheme eval_mutual_ind from
  eval_expr_mut_ind, eval_exprs_mut_ind, eval_stmt_mut_ind, eval_stmts_mut_ind, eval_for_mut_ind.

(* ------------------------------------------------------------------ *)
(** ** Evaluation of a complete program *)

(** [eval_program p inputs result]: run program [p] with the given
    input values and obtain [result]. *)
Definition eval_program (p : program) (inputs : val_env) (v : value) : Prop :=
  let Φ := func_val_env_of p in
  match List.find (fun fd => String.eqb (fd_name fd) "main") (prog_funcs p) with
  | None    => False
  | Some fd =>
      eval_stmts Φ inputs (fd_body fd) (ReturnVal v)
  end.
