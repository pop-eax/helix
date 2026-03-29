From Stdlib Require Import ZArith.
From Stdlib Require Import Lists.List.
From Stdlib Require Import Strings.String.
Import ListNotations.

From HelixDSL Require Import Types.
From HelixDSL Require Import Values.
From HelixDSL Require Import Syntax.
From HelixDSL Require Import Typing.
From HelixDSL Require Import Semantics.

(** * Helix DSL — Consistency and Soundness

    This file states the two central meta-theoretic properties of the
    Helix type system.  All theorems are [Admitted]; proofs are to be
    carried out manually.

    ┌─────────────────────────────────────────────────────────────────┐
    │  CONSISTENCY                                                    │
    │    The typing relation is well-defined: every expression has    │
    │    at most one type, and the type-checking rules are coherent.  │
    │                                                                 │
    │  SOUNDNESS                                                      │
    │    A well-typed expression evaluated in a consistent runtime    │
    │    environment produces a value of the declared type            │
    │    ("well-typed programs don't go wrong").                      │
    └─────────────────────────────────────────────────────────────────┘ *)

(* ================================================================== *)
(** ** §1  Environment consistency *)

(** [env_consistent ρ Γ]: the value environment [ρ] is consistent with
    the typing environment [Γ] — every variable that [Γ] assigns a type
    to is bound in [ρ] to a well-formed value of that type. *)
Definition env_consistent (ρ : val_env) (Γ : ty_env) : Prop :=
  forall x τ,
    ty_lookup Γ x = Some τ ->
    exists v, val_lookup ρ x = Some v /\ wf_value v τ.

(** [func_envs_consistent Φ_ty Φ_val]: the type environment [Φ_ty] and
    the runtime environment [Φ_val] agree on function arities. *)
Definition func_envs_consistent
    (Φ_ty  : func_ty_env)
    (Φ_val : func_val_env) : Prop :=
  forall f τ_params τ_ret,
    func_ty_lookup Φ_ty f = Some (τ_params, τ_ret) ->
    exists fv,
      func_val_lookup Φ_val f = Some fv /\
      length (fv_params fv) = length τ_params.

(* ================================================================== *)
(** ** §2  Consistency theorems *)

(** *** 2.1  Type uniqueness

    The typing judgment is a (partial) function on expressions: under
    any fixed environment, an expression has at most one type.  This
    rules out ambiguity or contradiction in the typing rules. *)

Theorem type_uniqueness :
  forall Γ Φ Σ e τ1 τ2,
    has_type_expr Γ Φ Σ e τ1 ->
    has_type_expr Γ Φ Σ e τ2 ->
    τ1 = τ2.
Proof.
  (* Strategy: induction on the first derivation, inversion on the
     second.  The key cases are TyBinop/TyUnop where [binop_type] and
     [unop_type] are deterministic functions — injectivity of [Some]
     closes those goals.  TyVar follows from the determinism of
     [ty_lookup].  TyField uses the determinism of [struct_lookup] and
     [field_lookup]. *)
  Admitted.

(** *** 2.2  Operator typing determinism

    Helper lemmas supporting [type_uniqueness]: the operator result-type
    functions are deterministic. *)

Lemma binop_type_det :
  forall op τ τ1 τ2,
    binop_type op τ = Some τ1 ->
    binop_type op τ = Some τ2 ->
    τ1 = τ2.
Proof.
  intros op τ τ1 τ2 H1 H2. rewrite H1 in H2. injection H2; auto.
Qed.

Lemma unop_type_det :
  forall op τ τ1 τ2,
    unop_type op τ = Some τ1 ->
    unop_type op τ = Some τ2 ->
    τ1 = τ2.
Proof.
  intros op τ τ1 τ2 H1 H2. rewrite H1 in H2. injection H2; auto.
Qed.

(** *** 2.3  Environment monotonicity

    Every statement extends the environment without removing bindings.
    Existing variables keep their types after a statement executes. *)

Theorem stmt_env_monotone :
  forall Φ Σ τ_ret Γ s Γ',
    has_type_stmt Φ Σ τ_ret Γ s Γ' ->
    forall x τ, ty_lookup Γ x = Some τ -> ty_lookup Γ' x = Some τ.
Proof.
  (* By induction on [has_type_stmt].
     - [TyLet]: the new environment is [(x,τ) :: Γ]; if [y ≠ x] the
       lookup falls through to [Γ].
     - [TyAssign], [TyReturn], [TyFor]: [Γ' = Γ], trivial.
     - [TyIf]: [Γ' = Γ], trivial. *)
  Admitted.

Theorem stmts_env_monotone :
  forall Φ Σ τ_ret Γ ss Γ',
    has_type_stmts Φ Σ τ_ret Γ ss Γ' ->
    forall x τ, ty_lookup Γ x = Some τ -> ty_lookup Γ' x = Some τ.
Proof.
  (* By induction on [has_type_stmts], using [stmt_env_monotone] at
     each step. *)
  Admitted.

(** *** 2.4  Consistency of the empty program

    The empty program is well-typed — the type system is satisfiable. *)

Lemma empty_program_consistent :
  wf_program (MkProgram []) [].
Proof.
  unfold wf_program. simpl. constructor.
Qed.

(* ================================================================== *)
(** ** §3  Soundness: expressions *)

(** *** 3.1  Operator soundness

    If [eval_binop] succeeds on well-typed inputs, the result has the
    declared type. *)

Lemma binop_soundness :
  forall op v1 v2 τ τ_res v,
    wf_value v1 τ ->
    wf_value v2 τ ->
    binop_type op τ = Some τ_res ->
    eval_binop op v1 v2 = Some v ->
    wf_value v τ_res.
Proof.
  (* By case analysis on [op] and [τ].  For arithmetic operators the
     result is [VField n _] and [to_field_range] supplies the
     well-formedness bound.  For logical operators the result is
     [VBool _], handled by [WfBool].  For comparisons the result is
     [VBool _]. *)
  Admitted.

Lemma unop_soundness :
  forall op v τ τ_res v',
    wf_value v τ ->
    unop_type op τ = Some τ_res ->
    eval_unop op v = Some v' ->
    wf_value v' τ_res.
Proof.
  Admitted.

(** *** 3.2  Expression soundness (main statement)

    If [e] is well-typed and is evaluated in a consistent environment,
    the result value is well-formed at the declared type. *)

Theorem expr_soundness :
  forall Φ_ty Φ_val Σ Γ ρ e τ v,
    env_consistent ρ Γ ->
    func_envs_consistent Φ_ty Φ_val ->
    has_type_expr Γ Φ_ty Σ e τ ->
    eval_expr Φ_val ρ e v ->
    wf_value v τ.
Proof.
  (* By mutual induction on the [eval_expr] derivation, using
     [type_uniqueness] at inversion points and the operator soundness
     lemmas for [EvalBinop] / [EvalUnop].

     Key cases:
     - [EvalVar]: [env_consistent] immediately gives [wf_value v τ].
     - [EvalConst]: [TyConst] already holds [wf_value v τ].
     - [EvalBinop]: use [binop_soundness].
     - [EvalSelectTrue/False]: the chosen branch has the same type; IH.
     - [EvalIndex]: [WfArray]'s [Forall] gives the element type.
     - [EvalCall]: the body is typed at [τ_ret]; soundness of [eval_stmts]
       with the callee's parameter environment closes the goal.
  *)
  Admitted.

(* ================================================================== *)
(** ** §4  Soundness: statements *)

(** *** 4.1  Environment preservation

    Executing a well-typed statement in a consistent environment yields
    a [Continue] result with a new environment that is consistent with
    the output typing environment. *)

Theorem stmt_soundness :
  forall Φ_ty Φ_val Σ Γ ρ τ_ret s Γ' r,
    env_consistent ρ Γ ->
    func_envs_consistent Φ_ty Φ_val ->
    has_type_stmt Φ_ty Σ τ_ret Γ s Γ' ->
    eval_stmt Φ_val ρ s r ->
    match r with
    | Continue ρ'  => env_consistent ρ' Γ'
    | ReturnVal v  => wf_value v τ_ret
    end.
Proof.
  (* By induction on [has_type_stmt].
     - [TyLet]: [expr_soundness] gives [wf_value v τ]; extend [ρ] with
       the new binding; show the extended env is consistent with
       [(x, τ) :: Γ'].
     - [TyAssign]: same, but the env is unchanged structurally (the
       new binding shadows the old; consistency is preserved because
       the new value has the same type as the old).
     - [TyIf]: [expr_soundness] for the condition; then either
       [stmts_soundness] for the then-branch or the else-branch.
     - [TyReturn]: [expr_soundness] gives [wf_value v τ_ret].
     - [TyFor]: each iteration is governed by [stmts_soundness]. *)
  Admitted.

Theorem stmts_soundness :
  forall Φ_ty Φ_val Σ Γ ρ τ_ret ss Γ' r,
    env_consistent ρ Γ ->
    func_envs_consistent Φ_ty Φ_val ->
    has_type_stmts Φ_ty Σ τ_ret Γ ss Γ' ->
    eval_stmts Φ_val ρ ss r ->
    match r with
    | Continue ρ'  => env_consistent ρ' Γ'
    | ReturnVal v  => wf_value v τ_ret
    end.
Proof.
  (* By induction on [has_type_stmts], using [stmt_soundness] for each
     step and the induction hypothesis for the tail. *)
  Admitted.

(* ================================================================== *)
(** ** §5  Progress

    A well-typed expression in a consistent, call-free, division-free
    context always evaluates to some value (it never gets stuck).

    We restrict to [no_calls] and [no_div] because:
    - [ECall] requires a matching function in [Φ_val]; inlining
      eliminates calls before evaluation.
    - Division by zero is a legitimate runtime error for which the
      language provides no static guarantee. *)

Theorem expr_progress :
  forall Φ_ty Φ_val Σ Γ ρ e τ,
    env_consistent ρ Γ ->
    func_envs_consistent Φ_ty Φ_val ->
    has_type_expr Γ Φ_ty Σ e τ ->
    no_calls e ->
    no_div   e ->
    exists v, eval_expr Φ_val ρ e v.
Proof.
  (* By induction on [has_type_expr].
     - [TyVar]: [env_consistent] supplies the value.
     - [TyConst]: the constant value witnesses the existential.
     - [TyBinop]: IH gives [v1] and [v2]; then [binop_soundness] shows
       [eval_binop] returns [Some v] (division excluded by [no_div]).
     - [TySelect]: the condition evaluates to [VBool true] or
       [VBool false]; in both cases the chosen branch evaluates by IH.
     - [TyIndex]: the array evaluates to [VArray vs] by IH; the index
       bound [i < n] and [length vs = n] give [nth_error vs i = Some v].
     - [TyField]: the struct evaluates by IH; [List.find] succeeds
       because the field is in scope. *)
  Admitted.

(* ================================================================== *)
(** ** §7  Auxiliary: expressions mentioned in statements

    Used in [stmts_progress] to state that all sub-expressions are
    call-free and division-free. *)

Fixpoint stmts_exprs (s : stmt) : list expr :=
  match s with
  | SLet _ _ _ e    => [e]
  | SAssign _ e     => [e]
  | SIf e st se     => e :: List.concat (List.map stmts_exprs st)
                           ++ List.concat (List.map stmts_exprs se)
  | SReturn e       => [e]
  | SFor _ _ _ body => List.concat (List.map stmts_exprs body)
  end.

(** Progress lifts to statement lists: under the same restrictions,
    execution of a well-typed statement list either falls through or
    returns a value. *)

Theorem stmts_progress :
  forall Φ_ty Φ_val Σ Γ ρ τ_ret ss Γ',
    env_consistent ρ Γ ->
    func_envs_consistent Φ_ty Φ_val ->
    has_type_stmts Φ_ty Σ τ_ret Γ ss Γ' ->
    Forall (fun s => forall e, In e (stmts_exprs s) -> no_calls e /\ no_div e) ss ->
    exists r, eval_stmts Φ_val ρ ss r.
Proof.
  Admitted.

(* ================================================================== *)
(** ** §6  Corollary: type safety of well-typed programs *)

(** Combining progress and soundness: a well-typed, call-free,
    division-free expression in a consistent environment evaluates
    to a value of the declared type and does not get stuck. *)

Corollary type_safety :
  forall Φ_ty Φ_val Σ Γ ρ e τ,
    env_consistent ρ Γ ->
    func_envs_consistent Φ_ty Φ_val ->
    has_type_expr Γ Φ_ty Σ e τ ->
    no_calls e ->
    no_div   e ->
    exists v, eval_expr Φ_val ρ e v /\ wf_value v τ.
Proof.
  intros * Henv Hfun Hty Hnc Hnd.
  destruct (expr_progress _ _ _ _ _ _ _ Henv Hfun Hty Hnc Hnd) as [v Heval].
  exists v. split.
  - exact Heval.
  - exact (expr_soundness _ _ _ _ _ _ _ _ Henv Hfun Hty Heval).
Qed.
