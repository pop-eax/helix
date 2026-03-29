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
  | VArray  : list value -> value
    (** A fixed-size array.  Well-formed arrays are homogeneous. *)
  | VStruct : list (string * value) -> value.
    (** A struct: an ordered list of (field_name, value) pairs. *)

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

    [wf_value v τ] asserts that the value [v] has type [τ] and that
    all sub-values are within their valid ranges. *)

Inductive wf_value : value -> ty -> Prop :=

  | WfField : forall n z,
      0 <= z < field_mod n ->
      wf_value (VField n z) (TField n)

  | WfBool : forall b,
      wf_value (VBool b) TBoolTy

  | WfArray : forall vs τ n,
      List.length vs = n ->
      Forall (fun v => wf_value v τ) vs ->
      wf_value (VArray vs) (TArray τ n)

  | WfStruct : forall fields sname,
      (** Struct values are well-formed for any struct type with the
          same name; field-level consistency is tracked by the struct
          definition environment (out of scope here). *)
      wf_value (VStruct fields) (TStruct sname).

(* ------------------------------------------------------------------ *)
(** ** Helpers *)

(** Extract the type of a value (defined for base values only;
    arrays and structs require external context for completeness). *)
Definition typeof_base_val (v : value) : option ty :=
  match v with
  | VField n _ => Some (TField n)
  | VBool _    => Some TBoolTy
  | _          => None
  end.
