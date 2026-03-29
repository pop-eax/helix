From Stdlib Require Import Arith.
From Stdlib Require Import Strings.String.

(** * Helix DSL — Type System

    Types in Helix govern the sizes of field elements, the shape of
    composite data, and (via visibility) how data may be shared in an
    MPC protocol.  This file defines the syntax of types and a few
    basic lemmas about them. *)

(* ------------------------------------------------------------------ *)
(** ** Bit-widths *)

(** [field_size] is a natural number representing the bit-width of a
    field element.  For example, [Field<64>] in the surface syntax
    corresponds to [BTField 64]. *)
Definition field_size := nat.

(* ------------------------------------------------------------------ *)
(** ** Base types *)

Inductive base_ty : Type :=
  | BTField : field_size -> base_ty
    (** n-bit unsigned integer, arithmetic performed mod 2^n. *)
  | BTBool  : base_ty.
    (** Boolean: exactly [true] or [false]. *)

(* ------------------------------------------------------------------ *)
(** ** Type expressions *)

Inductive ty : Type :=
  | TBase   : base_ty -> ty
    (** A primitive base type. *)
  | TArray  : ty -> nat -> ty
    (** [TArray τ n]: a homogeneous fixed-size array of [n] elements,
        each of type [τ].  Array sizes are always statically known. *)
  | TStruct : string -> ty.
    (** [TStruct name]: a named struct, identified nominally.
        Struct definitions are held in a separate environment. *)

(** Convenience shorthands matching the surface-syntax keywords. *)
Definition TField (n : field_size) : ty := TBase (BTField n).
Definition TBoolTy                 : ty := TBase BTBool.

(* ------------------------------------------------------------------ *)
(** ** Visibility qualifiers *)

(** Every variable and function parameter carries a visibility label
    that tracks whether the value is known to all parties ([Public])
    or is private to its owner ([Secret]). *)
Inductive visibility : Type :=
  | Public : visibility
  | Secret : visibility.

(** Lattice join: a result is secret whenever any input is secret. *)
Definition vis_join (v1 v2 : visibility) : visibility :=
  match v1, v2 with
  | Public, Public => Public
  | _,      _      => Secret
  end.

(* ------------------------------------------------------------------ *)
(** ** Decidable equality *)

Lemma base_ty_eq_dec : forall (b1 b2 : base_ty), {b1 = b2} + {b1 <> b2}.
Proof.
  decide equality.
  apply Nat.eq_dec.
Defined.

Lemma ty_eq_dec : forall (τ1 τ2 : ty), {τ1 = τ2} + {τ1 <> τ2}.
Proof.
  decide equality.
  - apply base_ty_eq_dec.
  - apply Nat.eq_dec.
  - apply String.string_dec.
Defined.

Definition ty_eqb (τ1 τ2 : ty) : bool :=
  if ty_eq_dec τ1 τ2 then true else false.

Lemma ty_eqb_iff : forall τ1 τ2, ty_eqb τ1 τ2 = true <-> τ1 = τ2.
Proof.
  intros τ1 τ2. unfold ty_eqb.
  destruct (ty_eq_dec τ1 τ2) as [Heq | Hne].
  - split; [intros _; exact Heq | intros _; reflexivity].
  - split; [intros H; discriminate H | intros H; contradiction].
Qed.
