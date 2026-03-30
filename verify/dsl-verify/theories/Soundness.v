From Stdlib Require Import ZArith.
From Stdlib Require Import Lists.List.
From Stdlib Require Import Strings.String.
From Stdlib Require Import Lia.
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

(** [func_envs_consistent Φ_ty Φ_val Σ]: the type environment [Φ_ty] and
    the runtime environment [Φ_val] agree on function arities, and each
    runtime function body is well-typed against the corresponding type
    signature. *)
Definition func_envs_consistent
    (Φ_ty  : func_ty_env)
    (Φ_val : func_val_env)
    (Σ     : struct_env) : Prop :=
  forall f τ_params τ_ret,
    func_ty_lookup Φ_ty f = Some (τ_params, τ_ret) ->
    exists fv Γ_out,
      func_val_lookup Φ_val f = Some fv /\
      List.length (fv_params fv) = List.length τ_params /\
      has_type_stmts Φ_ty Σ τ_ret
        (List.combine (fv_params fv) τ_params)
        (fv_body fv)
        Γ_out.

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

Lemma wf_value_type_unique :
  forall v τ1 τ2,
    wf_value v τ1 ->
    wf_value v τ2 ->
    τ1 = τ2.
Proof.
  intros v τ1 τ2 Hwf1 Hwf2.
  destruct Hwf1;
    inversion Hwf2; subst; try reflexivity;
    match goal with
    | Heq : _ = _ |- _ => inversion Heq; reflexivity
    end.
Qed.

(** *** 2.3  Environment consistency helpers

    Because environments are shadowing lists, a [let] does not preserve
    the exact lookup result for the rebound variable name.  The right
    helper invariant is therefore consistency of runtime environments
    with typing environments, not pointwise equality of lookups. *)

Lemma val_lookup_update_eq :
  forall ρ x v,
    val_lookup (val_update ρ x v) x = Some v.
Proof.
  intros ρ x v. unfold val_update. simpl.
  rewrite String.eqb_refl. reflexivity.
Qed.

Lemma val_lookup_update_neq :
  forall ρ x y v,
    x <> y ->
    val_lookup (val_update ρ y v) x = val_lookup ρ x.
Proof.
  intros ρ x y v Hneq. unfold val_update. simpl.
  destruct (String.eqb x y) eqn:Heq.
  - apply String.eqb_eq in Heq. contradiction.
  - reflexivity.
Qed.

Lemma env_consistent_update :
  forall ρ Γ x τ v,
    env_consistent ρ Γ ->
    wf_value v τ ->
    env_consistent (val_update ρ x v) ((x, τ) :: Γ).
Proof.
  unfold env_consistent.
  intros ρ Γ x τ v Henv Hwf y τ' Hy.
  simpl in Hy.
  destruct (String.eqb y x) eqn:Heq.
  - apply String.eqb_eq in Heq. subst.
    inversion Hy; subst.
    exists v. split.
    + apply val_lookup_update_eq.
    + exact Hwf.
  - apply Henv in Hy as [v' [Hlk Hwf']].
    exists v'. split.
    + rewrite val_lookup_update_neq.
      * exact Hlk.
      * intro Heq'. subst. rewrite String.eqb_refl in Heq. discriminate.
    + exact Hwf'.
Qed.

(** *** 2.4  Consistency of the empty program

    The empty program is well-typed — the type system is satisfiable. *)

Lemma empty_program_consistent :
  wf_program (MkProgram []) [].
Proof.
  unfold wf_program. simpl. constructor.
Qed.


Lemma eval_binop_div_nonzero :
  forall n a b v,
    eval_binop OpDiv (VField n a) (VField n b) = Some v ->
    b <> 0.
Proof.
  intros.
  unfold eval_binop in H.
    rewrite Nat.eqb_refl in H.
    destruct (Z.eqb b 0) eqn:Hbz.
    +  discriminate.
    + apply Z.eqb_neq in Hbz.
      assumption.
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
  intros op v1 v2 τ τ_res v Hwf1 Hwf2 Hty Heval.
  destruct op; inversion Hwf1; subst; inversion Hwf2; subst; try discriminate.
  -

    simpl in Heval; rewrite Nat.eqb_refl in Heval.
    inversion Hty; subst.
    inversion Heval; subst.
    apply WfField.
    apply Z.mod_pos_bound.
    apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
  - simpl in Heval; rewrite Nat.eqb_refl in Heval.
    inversion Hty; subst.
    inversion Heval; subst.
    apply WfField.
    apply Z.mod_pos_bound.
    apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
  - simpl in Heval; rewrite Nat.eqb_refl in Heval.
    inversion Hty; subst.
    inversion Heval; subst.
    apply WfField.
    apply Z.mod_pos_bound.
    apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
  - destruct (z0 =? 0).
    simpl in Heval.
    rewrite Nat.eqb_refl in Heval.
    destruct (Z.eqb z0 0) eqn:Hz0eq in Heval.

    + discriminate.
    + apply Z.eqb_neq in Hz0eq.
      inversion Hty; subst.
      inversion Heval;subst.
      apply WfField.
      assert (Hz0_pos : 0 < z0) by lia.
      split.
      * apply Z.div_pos; try lia.
      * assert (Hle : z / z0 <= z).
        { 
          apply Z.div_le_upper_bound.
          - assumption.
          - assert (Hone : 1 <= z0) by lia.
            rewrite <- Z.mul_1_l at 1.
            apply Z.mul_le_mono_nonneg_r; lia.
        }
      lia.
    + inversion Hty; subst. 
      inversion Heval; subst.
      rewrite Nat.eqb_refl in H1.
      destruct (Z.eqb z0 0) eqn:Hz0eq in Heval.
      * rewrite Hz0eq in H1; discriminate.
      * rewrite Hz0eq in H1.
        inversion H1; subst.
        apply Z.eqb_neq in Hz0eq.
        apply WfField.
        assert (Hz0_pos : 0 < z0) by lia.
        split.
        -- apply Z.div_pos; lia.
        -- assert (Hle : z / z0 <= z).
        {
          apply Z.div_le_upper_bound; try lia.
          rewrite <- Z.mul_1_l at 1.
          apply Z.mul_le_mono_nonneg_r; lia.
        }
        lia.
  - inversion Hty; subst.
    inversion Heval; subst.
    rewrite Nat.eqb_refl in H1.
    destruct (Z.eqb z0 0) eqn:Hz0eq in Heval.
    + rewrite Hz0eq in H1; discriminate.
    + rewrite Hz0eq in H1.
      inversion H1; subst.
      apply Z.eqb_neq in Hz0eq.
      apply WfField.
      assert (Hz0_pos : 0 < z0) by lia.
      assert (Hmod : 0 <= z mod z0 < z0).
      { apply Z.mod_pos_bound. exact Hz0_pos. }
      lia.
  - inversion Hty; subst.
    inversion Heval; subst.
    rewrite Nat.eqb_refl in H1.
    inversion H1; subst.
    apply WfField.
    split.
    + apply Z.land_nonneg; lia.
    + destruct (Z.eq_dec z 0) as [Hz0 | Hz0].
      * subst z.
        rewrite Z.land_0_l.
        unfold field_mod.
        apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
      * assert (Hz_pos : 0 < z) by lia.
        assert (Hz_log : Z.log2 z < Z.of_nat n).
        {
          apply Z.log2_lt_pow2; unfold field_mod in H; lia.
        }
        assert (Hland_log : Z.log2 (Z.land z z0) < Z.of_nat n).
        {
          eapply Z.le_lt_trans.
          - apply Z.log2_land; lia.
          - eapply Z.le_lt_trans.
            + apply Z.le_min_l.
            + exact Hz_log.
        }
        destruct (Z.eq_dec (Z.land z z0) 0) as [Hland0 | Hland0].
        -- rewrite Hland0.
            lia.
        -- assert (Hland_pos : 0 < Z.land z z0).
          {
            assert (Hland_nonneg : 0 <= Z.land z z0).
            { apply Z.land_nonneg. lia. }
            lia.
          }
          unfold field_mod.
          eapply Z.log2_lt_pow2; eauto.
    - inversion Hty; subst.
      inversion Heval; subst.
      rewrite Nat.eqb_refl in H1.
      inversion H1; subst.
      apply WfField.
      split.
      + apply Z.lor_nonneg. lia.
      + destruct (Z.eq_dec z 0) as [Hz0 | Hz0].
        * subst z.
          rewrite Z.lor_0_l.
          lia.
        * destruct (Z.eq_dec z0 0) as [Hz00 | Hz00].
          -- subst z0.
             rewrite Z.lor_0_r.
             lia.
          -- assert (Hz_pos : 0 < z) by lia.
             assert (Hz0_pos : 0 < z0) by lia.
             assert (Hz_log : Z.log2 z < Z.of_nat n).
             {
               apply Z.log2_lt_pow2; unfold field_mod in H; lia.
             }
             assert (Hz0_log : Z.log2 z0 < Z.of_nat n).
             {
               apply Z.log2_lt_pow2; unfold field_mod in H2; lia.
             }
             assert (Hlor_log : Z.log2 (Z.lor z z0) < Z.of_nat n).
             {
               rewrite Z.log2_lor by lia.
               apply Z.max_case_strong; lia.
             }
             destruct (Z.eq_dec (Z.lor z z0) 0) as [Hlor0 | Hlor0].
             ++ rewrite Hlor0. unfold field_mod.
                apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
             ++ assert (Hlor_nonneg : 0 <= Z.lor z z0).
                { apply Z.lor_nonneg. lia. }
                assert (Hlor_pos : 0 < Z.lor z z0) by lia.
                unfold field_mod.
                eapply Z.log2_lt_pow2; eauto.
    - inversion Hty; subst.
      inversion Heval; subst.
      rewrite Nat.eqb_refl in H1.
      inversion H1; subst.
      apply WfField.
      split.
      + apply Z.lxor_nonneg. lia.
      + destruct (Z.eq_dec z 0) as [Hz0 | Hz0].
        * subst z.
          rewrite Z.lxor_0_l.
          lia.
        * destruct (Z.eq_dec z0 0) as [Hz00 | Hz00].
          -- subst z0.
             rewrite Z.lxor_0_r.
             lia.
          -- assert (Hz_pos : 0 < z) by lia.
             assert (Hz0_pos : 0 < z0) by lia.
             assert (Hz_log : Z.log2 z < Z.of_nat n).
             {
               apply Z.log2_lt_pow2; unfold field_mod in H; lia.
             }
             assert (Hz0_log : Z.log2 z0 < Z.of_nat n).
             {
               apply Z.log2_lt_pow2; unfold field_mod in H2; lia.
             }
             assert (Hxor_log : Z.log2 (Z.lxor z z0) < Z.of_nat n).
             {
               assert (Hxor_le :
                 Z.log2 (Z.lxor z z0) <= Z.max (Z.log2 z) (Z.log2 z0)).
               { apply Z.log2_lxor; lia. }
               assert (Hmax_lt : Z.max (Z.log2 z) (Z.log2 z0) < Z.of_nat n).
               { apply Z.max_case_strong; lia. }
               lia.
             }
             destruct (Z.eq_dec (Z.lxor z z0) 0) as [Hxor0 | Hxor0].
             ++ rewrite Hxor0. unfold field_mod.
                apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
             ++ assert (Hxor_nonneg : 0 <= Z.lxor z z0).
                { apply Z.lxor_nonneg. lia. }
                assert (Hxor_pos : 0 < Z.lxor z z0) by lia.
                unfold field_mod.
                eapply Z.log2_lt_pow2; eauto.
    - simpl in Heval; rewrite Nat.eqb_refl in Heval.
      inversion Hty; subst.
      inversion Heval; subst.
      apply WfField.
      apply Z.mod_pos_bound.
      apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
    - inversion Hty; subst.
      inversion Heval; subst.
      rewrite Nat.eqb_refl in H1.
      inversion H1; subst.
      apply WfField.
      split.
      + apply Z.shiftr_nonneg. lia.
      + rewrite Z.shiftr_div_pow2 by lia.
        assert (Hpow_pos : 0 < 2 ^ z0).
        { apply Z.pow_pos_nonneg; lia. }
        assert (Hle : z / 2 ^ z0 <= z).
        {
          apply Z.div_le_upper_bound; try lia.
          rewrite <- Z.mul_1_l at 1.
          apply Z.mul_le_mono_nonneg_r; lia.
        }
        lia.
    - inversion Hty; subst.
      inversion Heval; subst.
      constructor.
    - inversion Hty; subst.
      inversion Heval; subst.
      constructor.
    - inversion Hty; subst.
      simpl in Heval.
      try rewrite Nat.eqb_refl in Heval.
      inversion Heval; subst.
      constructor.
    - inversion Hty; subst.
      simpl in Heval.
      try rewrite Nat.eqb_refl in Heval.
      inversion Heval; subst.
      constructor.
    - inversion Hty; subst.
      simpl in Heval.
      try rewrite Nat.eqb_refl in Heval.
      inversion Heval; subst.
      constructor.
    - inversion Hty; subst.
      simpl in Heval.
      try rewrite Nat.eqb_refl in Heval.
      inversion Heval; subst.
      constructor.
    - inversion Hty; subst.
      simpl in Heval.
      rewrite Nat.eqb_refl in Heval.
      inversion Heval; subst.
      constructor.
    - inversion Hty; subst.
      simpl in Heval.
      rewrite Nat.eqb_refl in Heval.
      inversion Heval; subst.
      constructor.
    - simpl in Hty, Heval.
      inversion Hty; subst.
      rewrite Nat.eqb_refl in Heval.
      inversion Heval; subst.
      constructor.
    - simpl in Hty, Heval.
      inversion Hty; subst.
      rewrite Nat.eqb_refl in Heval.
      inversion Heval; subst.
      constructor.
  Qed.

Lemma unop_soundness :
  forall op v τ τ_res v',
    wf_value v τ ->
    unop_type op τ = Some τ_res ->
    eval_unop op v = Some v' ->
    wf_value v' τ_res.
Proof.
  intros op v τ τ_res v' Hwf Hty Heval.
  destruct op; inversion Hwf; subst; simpl in Hty, Heval; try discriminate.
  - inversion Hty; subst.
    inversion Heval; subst.
    apply WfField.
    apply Z.mod_pos_bound.
    apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
  - inversion Hty; subst.
    inversion Heval; subst.
    constructor.
  - inversion Hty; subst.
    inversion Heval; subst.
    apply WfField.
    apply Z.mod_pos_bound.
    apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
Qed.

Lemma env_consistent_combine :
  forall xs τs vs,
    List.length xs = List.length τs ->
    Forall2 wf_value vs τs ->
    env_consistent (List.combine xs vs) (List.combine xs τs).
Proof.
  intros xs.
  induction xs as [|x xs IH]; intros τs vs Hlen Hwf.
  - destruct τs as [|τ τs']; [|discriminate].
    destruct vs as [|v vs']; [|inversion Hwf].
    unfold env_consistent.
    intros y τ Hy. inversion Hy.
  - destruct τs as [|τ τs']; [discriminate|].
    destruct vs as [|v vs']; [inversion Hwf|].
    inversion Hwf; subst.
    simpl in Hlen. inversion Hlen; subst. clear Hlen.
    unfold env_consistent in *.
    intros y τ0 Hy. simpl in Hy.
    destruct (String.eqb y x) eqn:Heq.
    + apply String.eqb_eq in Heq. subst.
      inversion Hy; subst.
      exists v. split.
      * simpl. rewrite String.eqb_refl. reflexivity.
      * exact H2.
    + specialize (IH τs' vs' H0 H4 y τ0 Hy).
      destruct IH as [v' [Hlk Hwfv]].
      exists v'. split.
      * simpl. rewrite Heq. exact Hlk.
      * exact Hwfv.
Qed.

(** *** 3.2  Expression soundness (main statement)

    If [e] is well-typed and is evaluated in a consistent environment,
    the result value is well-formed at the declared type. *)

Theorem expr_soundness :
  forall Φ_ty Φ_val Σ Γ ρ e τ v,
    env_consistent ρ Γ ->
    func_envs_consistent Φ_ty Φ_val Σ ->
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
    func_envs_consistent Φ_ty Φ_val Σ ->
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
    func_envs_consistent Φ_ty Φ_val Σ ->
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
    func_envs_consistent Φ_ty Φ_val Σ ->
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
    func_envs_consistent Φ_ty Φ_val Σ ->
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
    func_envs_consistent Φ_ty Φ_val Σ ->
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
