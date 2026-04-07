(** Helix fuzzing oracle runner.

    Reads a JSON test case from stdin, evaluates it using the extracted
    Coq [Eval.eval_program_c], and writes a JSON result to stdout.

    IMPORTANT: Coq extraction produces modules named List, String, etc.
    that shadow the OCaml stdlib modules.  All stdlib operations in this
    file use the [Stdlib.] prefix to avoid the shadowing. *)

(* ------------------------------------------------------------------ *)
(* String / char list conversion                                        *)
(* Coq extraction uses char list for all Coq strings.                  *)
(* ------------------------------------------------------------------ *)

let str_to_cl (s : string) : char list =
  let n = Stdlib.String.length s in
  let get i = Stdlib.String.get s i in
  let rec go i acc = if i < 0 then acc else go (i-1) (get i :: acc) in
  go (n-1) []

(* ------------------------------------------------------------------ *)
(* Minimal JSON lexer / parser                                         *)
(* All module references use Stdlib.* to avoid shadowing.             *)
(* ------------------------------------------------------------------ *)

type token =
  | TLBrace | TRBrace | TLBracket | TRBracket
  | TColon  | TComma
  | TString of string
  | TInt    of int
  | TTrue   | TFalse | TNull

let lex s =
  let n   = Stdlib.String.length s in
  let pos = ref 0 in
  let peek () = if !pos < n then Stdlib.Option.some (Stdlib.String.get s !pos) else None in
  let advance () = incr pos in
  let read_while f =
    let buf = Stdlib.Buffer.create 16 in
    let rec loop () =
      match peek () with
      | Some c when f c -> Stdlib.Buffer.add_char buf c; advance (); loop ()
      | _ -> ()
    in loop (); Stdlib.Buffer.contents buf
  in
  let tokens = ref [] in
  let emit t = tokens := t :: !tokens in
  while !pos < n do
    (match peek () with
     | None -> ()
     | Some ' ' | Some '\t' | Some '\n' | Some '\r' -> advance ()
     | Some '{' -> advance (); emit TLBrace
     | Some '}' -> advance (); emit TRBrace
     | Some '[' -> advance (); emit TLBracket
     | Some ']' -> advance (); emit TRBracket
     | Some ':' -> advance (); emit TColon
     | Some ',' -> advance (); emit TComma
     | Some '"' ->
       advance ();
       let buf = Stdlib.Buffer.create 32 in
       let rec rd () =
         match peek () with
         | None -> ()
         | Some '"' -> advance ()
         | Some '\\' ->
           advance ();
           (match peek () with
            | Some '"'  -> advance (); Stdlib.Buffer.add_char buf '"';  rd ()
            | Some '\\' -> advance (); Stdlib.Buffer.add_char buf '\\'; rd ()
            | Some 'n'  -> advance (); Stdlib.Buffer.add_char buf '\n'; rd ()
            | Some 't'  -> advance (); Stdlib.Buffer.add_char buf '\t'; rd ()
            | _ -> rd ())
         | Some c -> advance (); Stdlib.Buffer.add_char buf c; rd ()
       in rd ();
       emit (TString (Stdlib.Buffer.contents buf))
     | Some 't' ->
       let w = read_while (fun c -> c >= 'a' && c <= 'z') in
       if w = "true" then emit TTrue else ()
     | Some 'f' ->
       let w = read_while (fun c -> c >= 'a' && c <= 'z') in
       if w = "false" then emit TFalse else ()
     | Some 'n' ->
       let w = read_while (fun c -> c >= 'a' && c <= 'z') in
       if w = "null" then emit TNull else ()
     | Some c when c >= '0' && c <= '9' || c = '-' ->
       let neg = if c = '-' then (advance (); true) else false in
       let digits = read_while (fun d -> d >= '0' && d <= '9') in
       let v = Stdlib.int_of_string digits in
       emit (TInt (if neg then -v else v))
     | Some _ -> advance ())
  done;
  Stdlib.List.rev !tokens

type json =
  | JObj    of (string * json) list
  | JArr    of json list
  | JStr    of string
  | JInt    of int
  | JBool   of bool
  | JNull

let rec parse_json toks =
  match toks with
  | TLBrace :: rest -> parse_obj rest []
  | TLBracket :: rest -> parse_arr rest []
  | TString s :: rest -> (JStr s, rest)
  | TInt i :: rest -> (JInt i, rest)
  | TTrue  :: rest -> (JBool true, rest)
  | TFalse :: rest -> (JBool false, rest)
  | TNull  :: rest -> (JNull, rest)
  | _ -> Stdlib.failwith "parse_json: unexpected token"

and parse_obj toks acc =
  match toks with
  | TRBrace :: rest -> (JObj (Stdlib.List.rev acc), rest)
  | TString k :: TColon :: rest ->
    let (v, rest2) = parse_json rest in
    let rest3 = (match rest2 with TComma :: r -> r | r -> r) in
    parse_obj rest3 ((k, v) :: acc)
  | TComma :: rest -> parse_obj rest acc
  | _ -> Stdlib.failwith "parse_obj: unexpected token"

and parse_arr toks acc =
  match toks with
  | TRBracket :: rest -> (JArr (Stdlib.List.rev acc), rest)
  | _ ->
    let (v, rest) = parse_json toks in
    let rest2 = (match rest with TComma :: r -> r | r -> r) in
    parse_arr rest2 (v :: acc)

let parse s =
  let toks = lex s in
  let (j, _) = parse_json toks in j

let obj_field j key =
  match j with
  | JObj fields ->
    (match Stdlib.List.assoc_opt key fields with
     | Some v -> v
     | None -> Stdlib.failwith ("missing field: " ^ key))
  | _ -> Stdlib.failwith ("expected object for key " ^ key)

let to_int j = match j with JInt i -> i | _ -> Stdlib.failwith "expected int"
let to_str j = match j with JStr s -> s | _ -> Stdlib.failwith "expected string"
let to_arr j = match j with JArr xs -> xs | _ -> Stdlib.failwith "expected array"

(* ------------------------------------------------------------------ *)
(* Build Coq/OCaml AST nodes from JSON                                 *)
(* ------------------------------------------------------------------ *)

let binop_of_string = function
  | "Add"  -> Syntax.OpAdd  | "Sub"  -> Syntax.OpSub  | "Mul"  -> Syntax.OpMul
  | "Div"  -> Syntax.OpDiv  | "Mod"  -> Syntax.OpMod
  | "BAnd" -> Syntax.OpBAnd | "BOr"  -> Syntax.OpBOr  | "BXor" -> Syntax.OpBXor
  | "Shl"  -> Syntax.OpShl  | "Shr"  -> Syntax.OpShr
  | "LAnd" -> Syntax.OpLAnd | "LOr"  -> Syntax.OpLOr
  | "Eq"   -> Syntax.OpEq   | "Neq"  -> Syntax.OpNeq
  | "Lt"   -> Syntax.OpLt   | "Le"   -> Syntax.OpLe
  | "Gt"   -> Syntax.OpGt   | "Ge"   -> Syntax.OpGe
  | s -> Stdlib.failwith ("unknown binop: " ^ s)

let unop_of_string = function
  | "Neg"  -> Syntax.OpNeg
  | "Not"  -> Syntax.OpNot
  | "BNot" -> Syntax.OpBNot
  | s -> Stdlib.failwith ("unknown unop: " ^ s)

let rec expr_of_json (field_size : int) j =
  let tag = to_str (obj_field j "t") in
  match tag with
  | "Var"   -> Syntax.EVar (str_to_cl (to_str (obj_field j "n")))
  | "Const" ->
    let v = to_int (obj_field j "v") in
    Syntax.EConst (Values.VField (field_size, v))
  | "BinOp" ->
    let op = binop_of_string (to_str (obj_field j "op")) in
    let l  = expr_of_json field_size (obj_field j "l") in
    let r  = expr_of_json field_size (obj_field j "r") in
    Syntax.EBinop (op, l, r)
  | "UnOp" ->
    let op = unop_of_string (to_str (obj_field j "op")) in
    let e  = expr_of_json field_size (obj_field j "e") in
    Syntax.EUnop (op, e)
  | t -> Stdlib.failwith ("unknown expr tag: " ^ t)

(* ------------------------------------------------------------------ *)
(* Main                                                                 *)
(* ------------------------------------------------------------------ *)

let () =
  let buf = Stdlib.Buffer.create 4096 in
  (try
     while true do
       Stdlib.Buffer.add_channel buf Stdlib.stdin 1024
     done
   with Stdlib.End_of_file -> ());
  let input = Stdlib.Buffer.contents buf in
  let j = parse input in

  let field_size  = to_int (obj_field j "field_size") in
  let param_names = Stdlib.List.map to_str (to_arr (obj_field j "params")) in
  let inputs_raw  = Stdlib.List.map to_int (to_arr (obj_field j "inputs")) in
  let lets_json   = to_arr (obj_field j "lets") in
  let ret_json    = obj_field j "ret" in

  let let_stmts =
    Stdlib.List.map (fun lj ->
      let name = to_str (obj_field lj "name") in
      let e    = expr_of_json field_size (obj_field lj "expr") in
      Syntax.SLet (Types.Public, Types.TBase (Types.BTField field_size),
                   str_to_cl name, e))
      lets_json
  in
  let ret_stmt = Syntax.SReturn (expr_of_json field_size ret_json) in
  let stmts = let_stmts @ [ret_stmt] in

  let params =
    Stdlib.List.map (fun n ->
      { Syntax.param_vis  = Types.Public
      ; Syntax.param_type = Types.TBase (Types.BTField field_size)
      ; Syntax.param_name = str_to_cl n })
      param_names
  in

  let fd : Syntax.func_def =
    { Syntax.fd_name   = str_to_cl "main"
    ; Syntax.fd_params = params
    ; Syntax.fd_ret_ty = Types.TBase (Types.BTField field_size)
    ; Syntax.fd_body   = stmts }
  in
  let prog : Syntax.program = [fd] in

  let env : Semantics.val_env =
    Stdlib.List.combine
      (Stdlib.List.map str_to_cl param_names)
      (Stdlib.List.map (fun v -> Values.VField (field_size, v)) inputs_raw)
  in

  let fuel = 100000 in
  (match Eval.eval_program_c fuel prog env with
   | None ->
     Stdlib.print_string "{\"err\":\"stuck\"}\n"
   | Some (Values.VField (_, v)) ->
     Stdlib.Printf.printf "{\"ok\":%d}\n" v
   | Some (Values.VBool b) ->
     Stdlib.Printf.printf "{\"ok\":%d}\n" (if b then 1 else 0)
   | Some _ ->
     Stdlib.print_string "{\"err\":\"aggregate-result\"}\n")
