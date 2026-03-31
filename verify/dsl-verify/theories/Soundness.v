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
    Helix type system.

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

(** [env_consistent ρ Γ Σ]: the value environment [ρ] is consistent with
    the typing environment [Γ] — every variable that [Γ] assigns a type
    to is bound in [ρ] to a well-formed value of that type. *)
Definition env_consistent (ρ : val_env) (Γ : ty_env) (Σ : struct_env) : Prop :=
  forall x τ,
    ty_lookup Γ x = Some τ ->
    exists v, val_lookup ρ x = Some v /\ wf_value Σ v τ.

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

Lemma wf_value_type_unique :
  forall Σ v τ1 τ2,
    wf_value Σ v τ1 ->
    wf_value Σ v τ2 ->
    τ1 = τ2.
Proof.
  intros Σ v τ1 τ2 Hwf1 Hwf2.
  destruct Hwf1;
    inversion Hwf2; subst; try reflexivity;
    match goal with
    | Heq : _ = _ |- _ => inversion Heq; reflexivity
    end.
Qed.

Theorem type_uniqueness :
  forall Γ Φ Σ e τ1 τ2,
    has_type_expr Γ Φ Σ e τ1 ->
    has_type_expr Γ Φ Σ e τ2 ->
    τ1 = τ2.
Proof.
  intros Γ Φ Σ e τ1 τ2 Hty1.
  revert τ2.
  induction Hty1; intros τ2 Hty2; inversion Hty2; subst.
  - match goal with
    | Hlk2 : ty_lookup _ _ = Some _ |- _ =>
        rewrite H in Hlk2; inversion Hlk2; reflexivity
    end.
  - eapply wf_value_type_unique; eauto.
  - match goal with
    | Hty : has_type_expr _ _ _ e1 _ |- _ =>
        specialize (IHHty1_1 _ Hty)
    end.
    subst.
    match goal with
    | Hres2 : binop_type _ _ = Some _ |- _ =>
        rewrite H in Hres2; inversion Hres2; reflexivity
    end.
  - match goal with
    | Hty : has_type_expr _ _ _ e _ |- _ =>
        specialize (IHHty1 _ Hty)
    end.
    subst.
    match goal with
    | Hres2 : unop_type _ _ = Some _ |- _ =>
        rewrite H in Hres2; inversion Hres2; reflexivity
    end.
  - match goal with
    | Hthen : has_type_expr _ _ _ e_then _ |- _ =>
        exact (IHHty1_2 _ Hthen)
    end.
  - match goal with
    | Harr : has_type_expr _ _ _ e_arr _ |- _ =>
        specialize (IHHty1 _ Harr)
    end.
    inversion IHHty1. reflexivity.
  - match goal with
    | Hstruct : has_type_expr _ _ _ e_struct _ |- _ =>
        specialize (IHHty1 _ Hstruct)
    end.
    inversion IHHty1; subst.
    match goal with
    | Hlookup1 : struct_lookup _ _ = Some ?defs1,
      Hlookup2 : struct_lookup _ _ = Some ?defs2 |- _ =>
        rewrite Hlookup1 in Hlookup2; inversion Hlookup2; subst defs2
    end.
    match goal with
    | Hfield1 : field_lookup _ _ = Some _,
      Hfield2 : field_lookup _ _ = Some _ |- _ =>
        rewrite Hfield1 in Hfield2; inversion Hfield2; reflexivity
    end.
  - match goal with
    | Hcall2 : func_ty_lookup _ _ = Some _ |- _ =>
        rewrite H in Hcall2; inversion Hcall2; reflexivity
    end.
Qed.

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
  forall ρ Γ Σ x τ v,
    env_consistent ρ Γ Σ ->
    wf_value Σ v τ ->
    env_consistent (val_update ρ x v) ((x, τ) :: Γ) Σ.
Proof.
  unfold env_consistent.
  intros ρ Γ Σ x τ v Henv Hwf y τ' Hy.
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

Lemma fresh_in_ty_env_neq :
  forall Γ x y τ,
    fresh_in_ty_env Γ x ->
    ty_lookup Γ y = Some τ ->
    x <> y.
Proof.
  intros Γ x y τ Hfresh Hlookup Heq.
  subst y. unfold fresh_in_ty_env in Hfresh.
  rewrite Hlookup in Hfresh. discriminate.
Qed.

Lemma ty_lookup_cons_preserve :
  forall Γ x τx y τ,
    fresh_in_ty_env Γ x ->
    ty_lookup Γ y = Some τ ->
    ty_lookup ((x, τx) :: Γ) y = Some τ.
Proof.
  intros Γ x τx y τ Hfresh Hlookup.
  simpl. destruct (String.eqb y x) eqn:Heq.
  - apply String.eqb_eq in Heq. subst y.
    exfalso. eapply fresh_in_ty_env_neq; eauto.
  - exact Hlookup.
Qed.

Lemma env_consistent_weaken :
  forall ρ Γ Γ' Σ,
    env_consistent ρ Γ' Σ ->
    (forall x τ, ty_lookup Γ x = Some τ -> ty_lookup Γ' x = Some τ) ->
    env_consistent ρ Γ Σ.
Proof.
  unfold env_consistent.
  intros ρ Γ Γ' Σ Henv Hsub x τ Hlookup.
  apply Henv.
  eapply Hsub; eauto.
Qed.

Lemma env_consistent_update_existing :
  forall ρ Γ Σ x τ v,
    env_consistent ρ Γ Σ ->
    ty_lookup Γ x = Some τ ->
    wf_value Σ v τ ->
    env_consistent (val_update ρ x v) Γ Σ.
Proof.
  unfold env_consistent.
  intros ρ Γ Σ x τ v Henv Hlookup_x Hwf y τ' Hlookup_y.
  destruct (String.eqb y x) eqn:Heq.
  - apply String.eqb_eq in Heq. subst y.
    rewrite Hlookup_x in Hlookup_y. inversion Hlookup_y; subst τ'.
    exists v. split.
    + apply val_lookup_update_eq.
    + exact Hwf.
  - apply Henv in Hlookup_y as [v' [Hval Hwf']].
    exists v'. split.
    + rewrite val_lookup_update_neq; eauto.
      intro Heq'. subst. rewrite String.eqb_refl in Heq. discriminate.
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
  forall Σ op v1 v2 τ τ_res v,
    wf_value Σ v1 τ ->
    wf_value Σ v2 τ ->
    binop_type op τ = Some τ_res ->
    eval_binop op v1 v2 = Some v ->
    wf_value Σ v τ_res.
Proof.
  intros Σ op v1 v2 τ τ_res v Hwf1 Hwf2 Hty Heval.
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
  forall Σ op v τ τ_res v',
    wf_value Σ v τ ->
    unop_type op τ = Some τ_res ->
    eval_unop op v = Some v' ->
    wf_value Σ v' τ_res.
Proof.
  intros Σ op v τ τ_res v' Hwf Hty Heval.
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
  forall Σ xs τs vs,
    List.length xs = List.length τs ->
    Forall2 (wf_value Σ) vs τs ->
    env_consistent (List.combine xs vs) (List.combine xs τs) Σ.
Proof.
  intros Σ xs.
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

Lemma has_type_stmt_lookup_preserved :
  forall Φ Σ τ_ret Γ s Γ',
    has_type_stmt Φ Σ τ_ret Γ s Γ' ->
    forall x τ, ty_lookup Γ x = Some τ -> ty_lookup Γ' x = Some τ
with has_type_stmts_lookup_preserved :
  forall Φ Σ τ_ret Γ ss Γ',
    has_type_stmts Φ Σ τ_ret Γ ss Γ' ->
    forall x τ, ty_lookup Γ x = Some τ -> ty_lookup Γ' x = Some τ.
Proof.
  - intros Φ Σ τ_ret Γ s Γ' Hty.
    induction Hty; intros y τ0 Hlookup.
    + eapply ty_lookup_cons_preserve; eauto.
    + exact Hlookup.
    + exact Hlookup.
    + exact Hlookup.
    + exact Hlookup.
  - intros Φ Σ τ_ret Γ ss Γ' Hty.
    induction Hty; intros y τ Hlookup.
    + exact Hlookup.
    + match goal with
      | IH : forall x τ, ty_lookup Γ' x = Some τ -> ty_lookup Γ'' x = Some τ |- _ =>
          eapply IH;
          eapply has_type_stmt_lookup_preserved; eauto
      end.
Qed.

Lemma nth_error_Forall :
  forall {A : Type} (P : A -> Prop) xs i x,
    Forall P xs ->
    nth_error xs i = Some x ->
    P x.
Proof.
  intros A P xs i x HForall Hnth.
  apply nth_error_In in Hnth.
  eapply Forall_forall; eauto.
Qed.

Lemma wf_struct_fields_lookup :
  forall Σ fields field_defs fname v τ,
    wf_struct_fields Σ fields field_defs ->
    List.find (fun p => String.eqb (fst p) fname) fields = Some (fname, v) ->
    field_lookup field_defs fname = Some τ ->
    wf_value Σ v τ.
Proof.
  intros Σ fields field_defs fname v τ Hwf_fields.
  induction Hwf_fields; intros Hfind Hlookup.
  - simpl in Hfind. discriminate.
  - simpl in Hfind, Hlookup.
    destruct (String.eqb fname0 fname) eqn:Heq.
    + apply String.eqb_eq in Heq. subst fname0.
      inversion Hfind; subst.
      rewrite String.eqb_refl in Hlookup.
      inversion Hlookup; subst.
      exact H.
    + apply String.eqb_neq in Heq.
      assert (Heq' : String.eqb fname fname0 = false).
      { apply String.eqb_neq. congruence. }
      rewrite Heq' in Hlookup.
      eapply IHHwf_fields; eauto.
Qed.

Lemma wf_struct_fields_find :
  forall Σ fields field_defs fname τ,
    wf_struct_fields Σ fields field_defs ->
    field_lookup field_defs fname = Some τ ->
    exists v,
      List.find (fun p => String.eqb (fst p) fname) fields = Some (fname, v).
Proof.
  intros Σ fields field_defs fname τ Hwf_fields.
  induction Hwf_fields; intros Hlookup.
  - simpl in Hlookup. discriminate.
  - simpl in Hlookup.
    destruct (String.eqb fname0 fname) eqn:Heq.
    + apply String.eqb_eq in Heq. subst fname0.
      exists v. simpl. rewrite String.eqb_refl. reflexivity.
    + apply String.eqb_neq in Heq.
      assert (Heq' : String.eqb fname fname0 = false).
      { apply String.eqb_neq. congruence. }
      rewrite Heq' in Hlookup.
      destruct (IHHwf_fields Hlookup) as [v' Hfind].
      exists v'. simpl.
      destruct (String.eqb fname0 fname) eqn:Heq0.
      * apply String.eqb_eq in Heq0. contradiction.
      * exact Hfind.
Qed.

Lemma unop_progress_helper :
  forall Σ op v τ τ_res,
    wf_value Σ v τ ->
    unop_type op τ = Some τ_res ->
    exists v', eval_unop op v = Some v'.
Proof.
  intros Σ op v τ τ_res Hwf Hty.
  destruct op; inversion Hwf; subst; simpl in Hty; try discriminate;
    inversion Hty; subst; simpl; eauto.
Qed.

Lemma wf_value_bool_inv :
  forall Σ v,
    wf_value Σ v TBoolTy ->
    exists b, v = VBool b.
Proof.
  intros Σ v Hwf.
  inversion Hwf; subst; try discriminate.
  eauto.
Qed.

Lemma binop_progress_helper :
  forall Σ op v1 v2 τ τ_res,
    wf_value Σ v1 τ ->
    wf_value Σ v2 τ ->
    binop_type op τ = Some τ_res ->
    op <> OpDiv ->
    op <> OpMod ->
    exists v, eval_binop op v1 v2 = Some v.
Proof.
  intros.
  destruct op; destruct v1; destruct v2; simpl; (try inversion H; subst;
      inversion H0; subst); try (inversion H; subst;
      inversion H1; subst); try (rewrite Nat.eqb_refl;
    eauto).
  - contradiction.
  - contradiction.
  - eauto.
  - eauto.
  - eauto.
  - eauto.
Qed.

Lemma soundness_mutual :
  forall Φ_ty Φ_val Σ,
    (forall ρ e v,
      eval_expr Φ_val ρ e v ->
      forall Γ τ,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        has_type_expr Γ Φ_ty Σ e τ ->
        wf_value Σ v τ) /\
    (forall ρ args arg_vals,
      eval_exprs Φ_val ρ args arg_vals ->
      forall Γ τ_params,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        Forall2 (fun e τ => has_type_expr Γ Φ_ty Σ e τ) args τ_params ->
        Forall2 (wf_value Σ) arg_vals τ_params) /\
    (forall ρ s r,
      eval_stmt Φ_val ρ s r ->
      forall Γ τ_ret Γ',
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        has_type_stmt Φ_ty Σ τ_ret Γ s Γ' ->
        match r with
        | Continue ρ' => env_consistent ρ' Γ' Σ
        | ReturnVal v => wf_value Σ v τ_ret
        end) /\
    (forall ρ ss r,
      eval_stmts Φ_val ρ ss r ->
      forall Γ τ_ret Γ',
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        has_type_stmts Φ_ty Σ τ_ret Γ ss Γ' ->
        match r with
        | Continue ρ' => env_consistent ρ' Γ' Σ
        | ReturnVal v => wf_value Σ v τ_ret
        end) /\
    (forall ρ x lo hi body r,
      eval_for Φ_val ρ x lo hi body r ->
      forall Γ τ_ret Γ_body,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        fresh_in_ty_env Γ x ->
        has_type_stmts Φ_ty Σ τ_ret ((x, TField 64%nat) :: Γ) body Γ_body ->
        match r with
        | Continue ρ' => env_consistent ρ' Γ Σ
        | ReturnVal v => wf_value Σ v τ_ret
        end).
Proof.
  intros Φ_ty Φ_val Σ.
  apply (eval_mutual_ind Φ_val
    (fun ρ e v _ =>
      forall Γ τ,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        has_type_expr Γ Φ_ty Σ e τ ->
        wf_value Σ v τ)
    (fun ρ args arg_vals _ =>
      forall Γ τ_params,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        Forall2 (fun e τ => has_type_expr Γ Φ_ty Σ e τ) args τ_params ->
        Forall2 (wf_value Σ) arg_vals τ_params)
    (fun ρ s r _ =>
      forall Γ τ_ret Γ',
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        has_type_stmt Φ_ty Σ τ_ret Γ s Γ' ->
        match r with
        | Continue ρ' => env_consistent ρ' Γ' Σ
        | ReturnVal v => wf_value Σ v τ_ret
        end)
    (fun ρ ss r _ =>
      forall Γ τ_ret Γ',
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        has_type_stmts Φ_ty Σ τ_ret Γ ss Γ' ->
        match r with
        | Continue ρ' => env_consistent ρ' Γ' Σ
        | ReturnVal v => wf_value Σ v τ_ret
        end)
    (fun ρ x lo hi body r _ =>
      forall Γ τ_ret Γ_body,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        fresh_in_ty_env Γ x ->
        has_type_stmts Φ_ty Σ τ_ret ((x, TField 64%nat) :: Γ) body Γ_body ->
        match r with
        | Continue ρ' => env_consistent ρ' Γ Σ
        | ReturnVal v => wf_value Σ v τ_ret
        end)).
  - intros ρ x v Hlookup Γ τ Henv _ Hty.
    inversion Hty; subst.
    match goal with
    | Hty_lookup : ty_lookup _ _ = Some _ |- _ =>
        destruct (Henv x τ Hty_lookup) as [v' [Hlookup' Hwf]]
    end.
    rewrite Hlookup in Hlookup'. inversion Hlookup'. subst. exact Hwf.
  - intros ρ v Γ τ _ _ Hty.
    inversion Hty; subst.
    match goal with
    | Hwf : wf_value Σ v τ |- _ => exact Hwf
    end.
  - intros ρ op e1 e2 v1 v2 v He1 IH1 He2 IH2 Heval Γ τ Henv Hfun Hty.
    inversion Hty; subst.
    match goal with
    | Hbin : binop_type op ?τ0 = Some ?τr |- _ =>
        eapply (binop_soundness Σ op v1 v2 τ0 τr v);
          [eapply IH1; eauto | eapply IH2; eauto | exact Hbin | exact Heval]
    end.
  - intros ρ op e v1 v He IH Heval Γ τ Henv Hfun Hty.
    inversion Hty; subst.
    match goal with
    | Hun : unop_type op ?τ0 = Some ?τr |- _ =>
        eapply (unop_soundness Σ op v1 τ0 τr v);
          [eapply IH; eauto | exact Hun | exact Heval]
    end.
  - intros ρ e_cond e_then e_else v Hecond _ Hethen IHthen Γ τ Henv Hfun Hty.
    inversion Hty; subst.
    eapply IHthen; eauto.
  - intros ρ e_cond e_then e_else v Hecond _ Heelse IHelse Γ τ Henv Hfun Hty.
    inversion Hty; subst.
    eapply IHelse; eauto.
  - intros ρ e_arr τ_arr i vs v He_arr IHe_arr Hnth Γ τ Henv Hfun Hty.
    inversion Hty; subst.
    pose proof (IHe_arr _ _ Henv Hfun H1) as Hwf_arr.
    inversion Hwf_arr; subst.
    eapply (nth_error_Forall (fun v => wf_value Σ v τ) vs i v); eauto.
  - intros ρ e_struct sname fname fields v He_struct IHe_struct Hfind Γ τ Henv Hfun Hty.
    inversion Hty; subst.
    pose proof (IHe_struct _ _ Henv Hfun H1) as Hwf_struct.
    inversion Hwf_struct; subst.
    match goal with
    | Hlookup_ty : struct_lookup _ _ = Some _,
      Hlookup_wf : struct_lookup _ _ = Some _ |- _ =>
        rewrite Hlookup_ty in Hlookup_wf; inversion Hlookup_wf; subst
    end.
    eapply wf_struct_fields_lookup; eauto.
  - intros ρ f args fv arg_vals v Hlookup_f Hlen_args Hevals IHvals Hbody_eval IHbody
      Γ τ Henv Hfun Hty.
    inversion Hty; subst.
    match goal with
    | Hfun_lookup : func_ty_lookup Φ_ty f = Some (?τ_params, ?τ_ret),
      Htys : Forall2 (fun e τ => has_type_expr Γ Φ_ty Σ e τ) args τ_params |- _ =>
        destruct (Hfun f τ_params τ_ret Hfun_lookup)
          as [fv' [Γ_out [Hlookup_f' [Hlen_params Hbody_ty]]]];
        rewrite Hlookup_f in Hlookup_f';
        inversion Hlookup_f'; subst fv'; clear Hlookup_f';
        pose proof (IHvals _ _ Henv Hfun Htys) as Hwf_args;
        assert (Henv_args :
          env_consistent (List.combine (fv_params fv) arg_vals)
                         (List.combine (fv_params fv) τ_params) Σ)
          by (eapply env_consistent_combine; eauto);
        exact (IHbody _ _ _ Henv_args Hfun Hbody_ty)
    end.
  - intros ρ Γ τ_params _ _ Htys. inversion Htys. constructor.
  - intros ρ e es v vs He IHe Hes IHes Γ τ_params Henv Hfun Htys.
    inversion Htys; subst.
    constructor.
    + eapply IHe; eauto.
    + eapply IHes; eauto.
  - intros ρ vis τ x e v He IHe Γ τ_ret Γ' Henv Hfun Hty.
    inversion Hty; subst.
    eapply env_consistent_update; eauto.
  - intros ρ x e v He IHe Γ τ_ret Γ' Henv Hfun Hty.
    inversion Hty; subst.
    eapply env_consistent_update_existing; eauto.
  - intros ρ e_cond s_then s_else r Hecond _ Hthen IHthen Γ τ_ret Γ' Henv Hfun Hty.
    inversion Hty; subst.
    destruct r as [ρ'|v].
    + eapply env_consistent_weaken.
      * eapply IHthen; eauto.
      * intros y τ Hy.
        eapply has_type_stmts_lookup_preserved; eauto.
    + eapply IHthen; eauto.
  - intros ρ e_cond s_then s_else r Hecond _ Helse IHelse Γ τ_ret Γ' Henv Hfun Hty.
    inversion Hty; subst.
    destruct r as [ρ'|v].
    + eapply env_consistent_weaken.
      * eapply IHelse; eauto.
      * intros y τ Hy.
        eapply has_type_stmts_lookup_preserved; eauto.
    + eapply IHelse; eauto.
  - intros ρ e v He IHe Γ τ_ret Γ' Henv Hfun Hty.
    inversion Hty; subst.
    eapply IHe; eauto.
  - intros ρ x lo hi body r Hfor IHfor Γ τ_ret Γ' Henv Hfun Hty.
    inversion Hty; subst.
    eapply IHfor; eauto.
  - intros ρ Γ τ_ret Γ' Henv _ Hty.
    inversion Hty; subst. exact Henv.
  - intros ρ ρ' s rest r Hs IHs Hrest IHrest Γ τ_ret Γ' Henv Hfun Hty.
    inversion Hty; subst.
    match goal with
    | Hs_ty : has_type_stmt Φ_ty Σ τ_ret Γ s ?Γ_mid |- _ =>
        assert (Henv_mid : env_consistent ρ' Γ_mid Σ)
          by (eapply IHs; eauto);
        eapply IHrest; eauto
    end.
  - intros ρ s rest v Hs IHs Γ τ_ret Γ' Henv Hfun Hty.
    inversion Hty; subst.
    eapply IHs; eauto.
  - intros ρ x lo hi body Hge Γ τ_ret Γ_body Henv _ _ _.
    exact Henv.
  - intros ρ ρ' x lo hi body r Hlt Hbody IHbody Hfor IHfor
      Γ τ_ret Γ_body Henv Hfun Hfresh Hbody_ty.
    assert (Hwf_loop :
      wf_value Σ (VField 64%nat (to_field 64%nat (Z.of_nat lo))) (TField 64%nat)).
    {
      apply WfField.
      apply to_field_range.
    }
    assert (Henv_loop :
      env_consistent
        (val_update ρ x (VField 64%nat (to_field 64%nat (Z.of_nat lo))))
        ((x, TField 64%nat) :: Γ) Σ).
    {
      eapply env_consistent_update; eauto.
    }
    assert (Henv_body : env_consistent ρ' Γ_body Σ).
    {
      eapply IHbody; eauto.
    }
    assert (Henv_outer : env_consistent ρ' Γ Σ).
    {
      eapply env_consistent_weaken.
      - exact Henv_body.
      - intros y τ Hy.
        eapply has_type_stmts_lookup_preserved; eauto.
        eapply ty_lookup_cons_preserve; eauto.
    }
    eapply IHfor; eauto.
  - intros ρ x lo hi body v Hlt Hbody IHbody Γ τ_ret Γ_body Henv Hfun Hfresh Hbody_ty.
    assert (Hwf_loop :
      wf_value Σ (VField 64%nat (to_field 64%nat (Z.of_nat lo))) (TField 64%nat)).
    {
      apply WfField.
      apply to_field_range.
    }
    assert (Henv_loop :
      env_consistent
        (val_update ρ x (VField 64%nat (to_field 64%nat (Z.of_nat lo))))
        ((x, TField 64%nat) :: Γ) Σ).
    {
      eapply env_consistent_update; eauto.
    }
    eapply IHbody; eauto.
Qed.

(** *** 3.2  Expression soundness (main statement)

    If [e] is well-typed and is evaluated in a consistent environment,
    the result value is well-formed at the declared type. *)

Theorem expr_soundness :
  forall Φ_ty Φ_val Σ Γ ρ e τ v,
    env_consistent ρ Γ Σ ->
    func_envs_consistent Φ_ty Φ_val Σ ->
    has_type_expr Γ Φ_ty Σ e τ ->
    eval_expr Φ_val ρ e v ->
    wf_value Σ v τ.
Proof.
  intros Φ_ty Φ_val Σ Γ ρ e τ v Henv Hfun Hty Heval.
  destruct (soundness_mutual Φ_ty Φ_val Σ)
    as [Hexpr [_ [_ [_ _]]]].
  eapply Hexpr; eauto.
Qed.

Theorem stmt_soundness :
  forall Φ_ty Φ_val Σ Γ ρ τ_ret s Γ' r,
    env_consistent ρ Γ Σ ->
    func_envs_consistent Φ_ty Φ_val Σ ->
    has_type_stmt Φ_ty Σ τ_ret Γ s Γ' ->
    eval_stmt Φ_val ρ s r ->
    match r with
    | Continue ρ'  => env_consistent ρ' Γ' Σ
    | ReturnVal v  => wf_value Σ v τ_ret
    end.
Proof.
  intros Φ_ty Φ_val Σ Γ ρ τ_ret s Γ' r Henv Hfun Hty Heval.
  destruct (soundness_mutual Φ_ty Φ_val Σ)
    as [_ [_ [Hstmt [_ _]]]].
  eapply Hstmt; eauto.
Qed.

Theorem stmts_soundness :
  forall Φ_ty Φ_val Σ Γ ρ τ_ret ss Γ' r,
    env_consistent ρ Γ Σ ->
    func_envs_consistent Φ_ty Φ_val Σ ->
    has_type_stmts Φ_ty Σ τ_ret Γ ss Γ' ->
    eval_stmts Φ_val ρ ss r ->
    match r with
    | Continue ρ'  => env_consistent ρ' Γ' Σ
    | ReturnVal v  => wf_value Σ v τ_ret
    end.
Proof.
  intros Φ_ty Φ_val Σ Γ ρ τ_ret ss Γ' r Henv Hfun Hty Heval.
  destruct (soundness_mutual Φ_ty Φ_val Σ)
    as [_ [_ [_ [Hstmts _]]]].
  eapply Hstmts; eauto.
Qed.

Lemma exprs_soundness_helper :
  forall Φ_ty Φ_val Σ Γ ρ args τ_params arg_vals,
    env_consistent ρ Γ Σ ->
    func_envs_consistent Φ_ty Φ_val Σ ->
    Forall2 (fun e τ => has_type_expr Γ Φ_ty Σ e τ) args τ_params ->
    eval_exprs Φ_val ρ args arg_vals ->
    Forall2 (wf_value Σ) arg_vals τ_params.
Proof.
  intros Φ_ty Φ_val Σ Γ ρ args τ_params arg_vals Henv Hfun Htys Hevals.
  revert arg_vals Hevals.
  induction Htys; intros arg_vals Hevals.
  - inversion Hevals. constructor.
  - inversion Hevals; subst.
    constructor.
    + eapply expr_soundness; eauto.
    + eapply IHHtys; eauto.
Qed.

(* ================================================================== *)
(** ** §4  Soundness: statements *)

(** *** 4.1  Environment preservation

    Executing a well-typed statement in a consistent environment yields
    a [Continue] result with a new environment that is consistent with
    the output typing environment. *)


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
    env_consistent ρ Γ Σ ->
    func_envs_consistent Φ_ty Φ_val Σ ->
    has_type_expr Γ Φ_ty Σ e τ ->
    no_calls e ->
    no_div   e ->
    exists v, eval_expr Φ_val ρ e v.
Proof.
  intros Φ_ty Φ_val Σ Γ ρ e τ Henv Hfun Hty.
  induction Hty; intros Hnc Hnd.
  - destruct (Henv x τ H) as [v [Hlookup _]].
    exists v. constructor. exact Hlookup.
  - exists v. constructor.
  - inversion Hnc as [| | ? ? ? Hnc1 Hnc2 | | | |]; subst; clear Hnc.
    inversion Hnd as [| | ? ? ? Hneq_div Hneq_mod Hd1 Hd2 | | | | |]; subst; clear Hnd.
    destruct (IHHty1 Hnc1 Hd1) as [v1 Heval1].
    destruct (IHHty2 Hnc2 Hd2) as [v2 Heval2].
    pose proof (expr_soundness Φ_ty Φ_val Σ Γ ρ e1 τ v1 Henv Hfun Hty1 Heval1) as Hwf1.
    pose proof (expr_soundness Φ_ty Φ_val Σ Γ ρ e2 τ v2 Henv Hfun Hty2 Heval2) as Hwf2.
    destruct (binop_progress_helper Σ op v1 v2 τ τ_res Hwf1 Hwf2 H Hneq_div Hneq_mod)
      as [v_res Heval_op].
    exists v_res. econstructor; eauto.
  - inversion Hnc; subst; clear Hnc.
    inversion Hnd; subst; clear Hnd.
    match goal with
    | Hnc_e : no_calls e, Hd_e : no_div e |- _ =>
        destruct (IHHty Hnc_e Hd_e) as [v1 Heval1];
        pose proof (expr_soundness Φ_ty Φ_val Σ Γ ρ e τ v1 Henv Hfun Hty Heval1) as Hwf1;
        destruct (unop_progress_helper Σ op v1 τ τ_res Hwf1 H) as [v_res Heval_op];
        exists v_res;
        econstructor; eauto
    end.
  - inversion Hnc; subst; clear Hnc.
    inversion Hnd; subst; clear Hnd.
    match goal with
    | Hnc_cond : no_calls e_cond,
      Hnc_then : no_calls e_then,
      Hnc_else : no_calls e_else,
      Hd_cond : no_div e_cond,
      Hd_then : no_div e_then,
      Hd_else : no_div e_else |- _ =>
        destruct (IHHty1 Hnc_cond Hd_cond) as [v_cond Heval_cond];
        pose proof (expr_soundness Φ_ty Φ_val Σ Γ ρ e_cond TBoolTy v_cond Henv Hfun Hty1 Heval_cond)
          as Hwf_cond;
        destruct (wf_value_bool_inv Σ v_cond Hwf_cond) as [b Hb];
        subst v_cond;
        destruct b;
        [destruct (IHHty2 Hnc_then Hd_then) as [v_then Heval_then];
         exists v_then; apply EvalSelectTrue; assumption
        |destruct (IHHty3 Hnc_else Hd_else) as [v_else Heval_else];
         exists v_else; apply EvalSelectFalse; assumption]
    end.
  - inversion Hnc as [| | | | | ? ? Hnc_arr |]; subst; clear Hnc.
    inversion Hnd as [| | | | | ? ? Hd_arr | |]; subst; clear Hnd.
    destruct (IHHty Hnc_arr Hd_arr) as [v_arr Heval_arr].
    pose proof (expr_soundness Φ_ty Φ_val Σ Γ ρ e_arr (TArray τ n) v_arr Henv Hfun Hty Heval_arr)
      as Hwf_arr.
    inversion Hwf_arr as [| | ? vs ? Hlen _ |]; subst.
    destruct (nth_error vs i) eqn:Hnth.
    + exists v. econstructor; eauto.
    + apply nth_error_None in Hnth. lia.
  - inversion Hnc as [| | | | | | ? ? Hnc_struct]; subst; clear Hnc.
    inversion Hnd as [| | | | | | ? ? Hd_struct |]; subst; clear Hnd.
    destruct (IHHty Hnc_struct Hd_struct) as [v_struct Heval_struct].
    pose proof (expr_soundness Φ_ty Φ_val Σ Γ ρ e_struct (TStruct sname) v_struct Henv Hfun Hty Heval_struct)
      as Hwf_struct.
    inversion Hwf_struct; subst.
    match goal with
    | Hlookup_ty : struct_lookup _ _ = Some _,
      Hlookup_wf : struct_lookup _ _ = Some _ |- _ =>
        rewrite Hlookup_ty in Hlookup_wf; inversion Hlookup_wf; subst
    end.
    match goal with
    | Hfields : wf_struct_fields Σ fields _,
      Hfield : field_lookup _ fname = Some τ_field |- _ =>
        eapply wf_struct_fields_find in Hfield; [| exact Hfields];
        destruct Hfield as [v_res Hfind];
        exists v_res;
        econstructor; eauto
    end.
  - inversion Hnc.
Qed.

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

Lemma in_concat_map :
  forall {A B : Type} (f : A -> list B) (xs : list A) (x : A) (y : B),
    In x xs ->
    In y (f x) ->
    In y (List.concat (List.map f xs)).
Proof.
  intros A B f xs x y Hinx Hiny.
  induction xs as [|x' xs IH]; simpl in *.
  - contradiction.
  - destruct Hinx as [-> | Hrest].
    + apply in_or_app. left. exact Hiny.
    + apply in_or_app. right. eapply IH; eauto.
Qed.

Scheme has_type_stmt_mut_ind := Induction for has_type_stmt Sort Prop
  with has_type_stmts_mut_ind := Induction for has_type_stmts Sort Prop.

Combined Scheme has_type_stmt_stmts_ind
  from has_type_stmt_mut_ind, has_type_stmts_mut_ind.

(** Progress lifts to statement lists: under the same restrictions,
    execution of a well-typed statement list either falls through or
    returns a value. *)

Lemma stmt_stmts_progress_mutual :
  forall Φ_ty Σ τ_ret,
    (forall Γ s Γ',
      has_type_stmt Φ_ty Σ τ_ret Γ s Γ' ->
      forall Φ_val ρ,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        (forall e, In e (stmts_exprs s) -> no_calls e /\ no_div e) ->
        exists r, eval_stmt Φ_val ρ s r) /\
    (forall Γ ss Γ',
      has_type_stmts Φ_ty Σ τ_ret Γ ss Γ' ->
      forall Φ_val ρ,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        Forall (fun s => forall e, In e (stmts_exprs s) -> no_calls e /\ no_div e) ss ->
        exists r, eval_stmts Φ_val ρ ss r).
Proof.
  intros Φ_ty Σ τ_ret.
  apply (has_type_stmt_stmts_ind Φ_ty Σ τ_ret
    (fun Γ s Γ' _ =>
      forall Φ_val ρ,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        (forall e, In e (stmts_exprs s) -> no_calls e /\ no_div e) ->
        exists r, eval_stmt Φ_val ρ s r)
    (fun Γ ss Γ' _ =>
      forall Φ_val ρ,
        env_consistent ρ Γ Σ ->
        func_envs_consistent Φ_ty Φ_val Σ ->
        Forall (fun s => forall e, In e (stmts_exprs s) -> no_calls e /\ no_div e) ss ->
        exists r, eval_stmts Φ_val ρ ss r)).
  - intros Γ vis τ x e Hfresh Hexpr Φ_val ρ Henv Hfun Hsafe.
    destruct (Hsafe e (or_introl eq_refl)) as [Hnc Hnd].
    destruct (expr_progress Φ_ty Φ_val Σ Γ ρ e τ Henv Hfun Hexpr Hnc Hnd)
      as [v Heval].
    exists (Continue (val_update ρ x v)).
    econstructor; eauto.
  - intros Γ x e τ Hlookup Hexpr Φ_val ρ Henv Hfun Hsafe.
    destruct (Hsafe e (or_introl eq_refl)) as [Hnc Hnd].
    destruct (expr_progress Φ_ty Φ_val Σ Γ ρ e τ Henv Hfun Hexpr Hnc Hnd)
      as [v Heval].
    exists (Continue (val_update ρ x v)).
    econstructor; eauto.
  - intros Γ e_cond s_then s_else Γ' Γ'' Hcond Hthen IHthen Helse IHelse
      Φ_val ρ Henv Hfun Hsafe.
    destruct (Hsafe e_cond) as [Hnc_cond Hnd_cond].
    {
      simpl. left. reflexivity.
    }
    destruct (expr_progress Φ_ty Φ_val Σ Γ ρ e_cond TBoolTy Henv Hfun Hcond Hnc_cond Hnd_cond)
      as [v_cond Heval_cond].
    pose proof (expr_soundness Φ_ty Φ_val Σ Γ ρ e_cond TBoolTy v_cond
                  Henv Hfun Hcond Heval_cond) as Hwf_cond.
    destruct (wf_value_bool_inv Σ v_cond Hwf_cond) as [b Hb].
    subst v_cond.
    destruct b.
    + assert (Hsafe_then :
        Forall (fun s => forall e, In e (stmts_exprs s) -> no_calls e /\ no_div e) s_then).
      {
        apply Forall_forall.
        intros s Hins e Hine.
        apply Hsafe.
        simpl. right. apply in_or_app. left.
        eapply in_concat_map; eauto.
      }
      destruct (IHthen Φ_val ρ Henv Hfun Hsafe_then) as [r Hr].
      exists r. apply EvalIfTrue; assumption.
    + assert (Hsafe_else :
        Forall (fun s => forall e, In e (stmts_exprs s) -> no_calls e /\ no_div e) s_else).
      {
        apply Forall_forall.
        intros s Hins e Hine.
        apply Hsafe.
        simpl. right. apply in_or_app. right.
        eapply in_concat_map; eauto.
      }
      destruct (IHelse Φ_val ρ Henv Hfun Hsafe_else) as [r Hr].
      exists r. apply EvalIfFalse; assumption.
  - intros Γ e Hexpr Φ_val ρ Henv Hfun Hsafe.
    destruct (Hsafe e (or_introl eq_refl)) as [Hnc Hnd].
    destruct (expr_progress Φ_ty Φ_val Σ Γ ρ e τ_ret Henv Hfun Hexpr Hnc Hnd)
      as [v Heval].
    exists (ReturnVal v).
    econstructor; eauto.
  - intros Γ x lo hi body Γ_body Hfresh Hbody IHbody Φ_val ρ Henv Hfun Hsafe.
    assert (Hsafe_body :
      Forall (fun s => forall e, In e (stmts_exprs s) -> no_calls e /\ no_div e) body).
    {
      apply Forall_forall.
      intros s Hins e Hine.
      apply Hsafe.
      simpl.
      eapply in_concat_map; eauto.
    }
    assert (Hfor_progress :
      forall fuel ρ0 lo0,
        fuel = (hi - lo0)%nat ->
        env_consistent ρ0 Γ Σ ->
        exists r, eval_for Φ_val ρ0 x lo0 hi body r).
    {
      intro fuel.
      induction fuel using lt_wf_ind.
      intros ρ0 lo0 Hfuel Henv0.
      destruct (le_gt_dec hi lo0) as [Hdone | Hstep].
      - exists (Continue ρ0).
        apply EvalForDone. lia.
      - assert (Hlt : (lo0 < hi)%nat) by lia.
        assert (Hwf_loop :
          wf_value Σ (VField 64%nat (to_field 64%nat (Z.of_nat lo0))) (TField 64%nat)).
        {
          apply WfField.
          apply to_field_range.
        }
        assert (Henv_loop :
          env_consistent
            (val_update ρ0 x (VField 64%nat (to_field 64%nat (Z.of_nat lo0))))
            ((x, TField 64%nat) :: Γ) Σ).
        {
          eapply env_consistent_update; eauto.
        }
        destruct (IHbody Φ_val
                   (val_update ρ0 x (VField 64%nat (to_field 64%nat (Z.of_nat lo0))))
                   Henv_loop Hfun Hsafe_body) as [r_body Hbody_eval].
        destruct r_body as [ρ1 | v].
        + assert (Henv_body : env_consistent ρ1 Γ_body Σ).
          {
            pose proof
              (stmts_soundness Φ_ty Φ_val Σ
                 ((x, TField 64%nat) :: Γ)
                 (val_update ρ0 x (VField 64%nat (to_field 64%nat (Z.of_nat lo0))))
                 τ_ret body Γ_body (Continue ρ1)
                 Henv_loop Hfun Hbody Hbody_eval) as Hsound.
            simpl in Hsound.
            exact Hsound.
          }
          assert (Henv_outer : env_consistent ρ1 Γ Σ).
          {
            eapply env_consistent_weaken.
            * exact Henv_body.
            * intros y τ Hlookup.
              eapply has_type_stmts_lookup_preserved; eauto.
              eapply ty_lookup_cons_preserve; eauto.
          }
          assert (Hsmaller : (hi - S lo0 < fuel)%nat) by lia.
          destruct (H (hi - S lo0)%nat Hsmaller ρ1 (S lo0) eq_refl Henv_outer)
            as [r Hfor_tail].
          exists r.
          eapply EvalForStep; eauto.
        + exists (ReturnVal v).
          eapply EvalForReturn; eauto.
    }
    destruct (Hfor_progress (hi - lo)%nat ρ lo eq_refl Henv) as [r Hr].
    exists r.
    apply EvalFor; exact Hr.
  - intros Γ Φ_val ρ Henv _ _.
    exists (Continue ρ).
    constructor.
  - intros Γ Γ' Γ'' s rest Hstmt IHstmt Hrest IHrest Φ_val ρ Henv Hfun Hsafe.
    inversion Hsafe; subst.
    destruct (IHstmt Φ_val ρ Henv Hfun H1) as [r1 Hr1].
    destruct r1 as [ρ' | v].
    + assert (Henv' : env_consistent ρ' Γ' Σ).
      {
        pose proof
          (stmt_soundness Φ_ty Φ_val Σ Γ ρ τ_ret s Γ' (Continue ρ')
             Henv Hfun Hstmt Hr1) as Hsound.
        simpl in Hsound.
        exact Hsound.
      }
      destruct (IHrest Φ_val ρ' Henv' Hfun H2) as [r2 Hr2].
      exists r2.
      econstructor; eauto.
    + exists (ReturnVal v).
      apply EvalConsReturn.
      exact Hr1.
Qed.

Theorem stmts_progress :
  forall Φ_ty Φ_val Σ Γ ρ τ_ret ss Γ',
    env_consistent ρ Γ Σ ->
    func_envs_consistent Φ_ty Φ_val Σ ->
    has_type_stmts Φ_ty Σ τ_ret Γ ss Γ' ->
    Forall (fun s => forall e, In e (stmts_exprs s) -> no_calls e /\ no_div e) ss ->
    exists r, eval_stmts Φ_val ρ ss r.
Proof.
  intros Φ_ty Φ_val Σ Γ ρ τ_ret ss Γ' Henv Hfun Hty Hsafe.
  destruct (stmt_stmts_progress_mutual Φ_ty Σ τ_ret) as [_ Hstmts].
  eapply Hstmts; eauto.
Qed.

(* ================================================================== *)
(** ** §6  Corollary: type safety of well-typed programs *)

(** Combining progress and soundness: a well-typed, call-free,
    division-free expression in a consistent environment evaluates
    to a value of the declared type and does not get stuck. *)

Corollary type_safety :
  forall Φ_ty Φ_val Σ Γ ρ e τ,
    env_consistent ρ Γ Σ ->
    func_envs_consistent Φ_ty Φ_val Σ ->
    has_type_expr Γ Φ_ty Σ e τ ->
    no_calls e ->
    no_div   e ->
    exists v, eval_expr Φ_val ρ e v /\ wf_value Σ v τ.
Proof.
  intros * Henv Hfun Hty Hnc Hnd.
  destruct (expr_progress _ _ _ _ _ _ _ Henv Hfun Hty Hnc Hnd) as [v Heval].
  exists v. split.
  - exact Heval.
  - exact (expr_soundness _ _ _ _ _ _ _ _ Henv Hfun Hty Heval).
Qed.
