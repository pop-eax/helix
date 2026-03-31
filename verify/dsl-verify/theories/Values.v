From Stdlib Require Import ZArith.
From Stdlib Require Import Lists.List.
From Stdlib Require Import Strings.String.
From Stdlib Require Import Bool.
From Stdlib Require Import Lia.
Import ListNotations.

From HelixDSL Require Import Types.

(** * Helix DSL — Runtime Values

    Values are the results of evaluating expressions.  Every value
    carries enough information to determine its own type (e.g., a
    field element stores its bit-width alongside the integer). *)

Open Scope Z_scope.

(* ------------------------------------------------------------------ *)
(** ** Value syntax *)

Inductive value : Type :=
  | VField  : field_size -> Z -> value
    (** [VField n z]: an n-bit field element with integer representative [z].
        Well-formed values satisfy [0 <= z < 2^n]. *)
  | VBool   : bool -> value
    (** A boolean value. *)
  | VArray  : ty -> list value -> value
    (** A fixed-size array tagged with its element type. *)
  | VStruct : string -> list (string * value) -> value.
    (** A struct tagged with its nominal type name and field values. *)

(* ------------------------------------------------------------------ *)
(** ** Field arithmetic *)

(** The modulus for an n-bit field: 2^n. *)
Definition field_mod (n : field_size) : Z := Z.pow 2 (Z.of_nat n).

(** Canonical reduction into the n-bit field (always non-negative). *)
Definition to_field (n : field_size) (z : Z) : Z :=
  ((z mod field_mod n) + field_mod n) mod field_mod n.

Lemma to_field_range : forall n z, 0 <= to_field n z < field_mod n.
Proof.
  intros n z. unfold to_field.
  apply Z.mod_pos_bound.
  apply Z.pow_pos_nonneg; [lia | apply Nat2Z.is_nonneg].
Qed.

(* ------------------------------------------------------------------ *)
(** ** Well-formed values

    [wf_value Σ v τ] asserts that the value [v] has type [τ] and that
    all sub-values are within their valid ranges and agree with the
    struct definitions from [Σ]. *)

Inductive wf_value (Σ : struct_env) : value -> ty -> Prop :=

  | WfField : forall n z,
      0 <= z < field_mod n ->
      wf_value Σ (VField n z) (TField n)

  | WfBool : forall b,
      wf_value Σ (VBool b) TBoolTy

  | WfArray : forall τ vs n,
      List.length vs = n ->
      Forall (fun v => wf_value Σ v τ) vs ->
      wf_value Σ (VArray τ vs) (TArray τ n)

  | WfStruct : forall sname fields field_defs,
      struct_lookup Σ sname = Some field_defs ->
      wf_struct_fields Σ fields field_defs ->
      wf_value Σ (VStruct sname fields) (TStruct sname)

with wf_struct_fields (Σ : struct_env)
    : list (string * value) -> list (string * ty) -> Prop :=
  | WfStructFieldsNil :
      wf_struct_fields Σ [] []
  | WfStructFieldsCons : forall fname v fields τ field_defs,
      wf_value Σ v τ ->
      wf_struct_fields Σ fields field_defs ->
      wf_struct_fields Σ ((fname, v) :: fields) ((fname, τ) :: field_defs).

(* ------------------------------------------------------------------ *)
(** ** Helpers *)

(** Extract the type of a value (defined for base values only;
    arrays and structs are tagged directly, but this helper stays
    focused on base values for now). *)
Definition typeof_base_val (v : value) : option ty :=
  match v with
  | VField n _ => Some (TField n)
  | VBool _    => Some TBoolTy
  | _          => None
  end.
