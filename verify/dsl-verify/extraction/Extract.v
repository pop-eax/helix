From Stdlib Require Import Extraction.
From Stdlib Require Import ExtrOcamlString.
From Stdlib Require Import ExtrOcamlNatInt.
From Stdlib Require Import ExtrOcamlZInt.

From HelixDSL Require Import Types.
From HelixDSL Require Import Values.
From HelixDSL Require Import Syntax.
From HelixDSL Require Import Semantics.
From HelixDSL Require Import Eval.

(** * OCaml Extraction for the Helix Computable Evaluator

    This file extracts [Eval.eval_program_c] to OCaml so that it can be
    compiled into the [helix_eval] oracle binary used by the fuzzing
    differential testing harness.

    Extraction strategy:
    - [string]  → OCaml [string]  (via ExtrOcamlString)
    - [nat]     → OCaml [int]     (via ExtrOcamlNatInt)
    - [Z]       → OCaml [int]     (via ExtrOcamlZInt)
    - [bool]    → OCaml [bool]    (built-in)
    - [list]    → OCaml ['a list] (built-in)
    - [option]  → OCaml ['a option] (built-in)

    The resulting ML files are placed in [extracted/] by the Makefile. *)

(** Place all extracted files in the [extracted/] subdirectory. *)
Set Extraction Output Directory "extracted".

(** Inline the [collect_opts] helper to avoid an extra module dependency. *)
Extraction Inline Eval.collect_opts.

(** Extract the entry-point and all transitive dependencies. *)
Recursive Extraction Library Eval.
