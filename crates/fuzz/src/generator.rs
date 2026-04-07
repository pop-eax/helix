/// Grammar-based test case generator.
///
/// Produces [TestCase] values that can be rendered as both `.mpc` source
/// (for the Rust pipeline) and JSON (for the Coq oracle).  The generator
/// only produces type-correct programs over a single field size with no
/// division or arrays, keeping both oracles in full agreement.
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

// ---- Types ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub field_size: u32,
    /// Parameter names (p0, p1, …).  Each maps to a `Public Field<N>` input.
    pub params: Vec<String>,
    /// Input values corresponding to each parameter (same order).
    pub inputs: Vec<u64>,
    /// Let-bindings: evaluated left-to-right, each may reference params.
    pub lets: Vec<LetBinding>,
    /// Return expression (may reference params + all let names).
    pub ret: Expr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetBinding {
    pub name: String,
    pub expr: Expr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Expr {
    /// Variable reference.
    #[serde(rename = "Var")]
    Var { n: String },
    /// Field-element constant.
    #[serde(rename = "Const")]
    Const { v: u64 },
    /// Binary operation.
    #[serde(rename = "BinOp")]
    BinOp {
        op: BinOp,
        l: Box<Expr>,
        r: Box<Expr>,
    },
    /// Unary operation.
    #[serde(rename = "UnOp")]
    UnOp { op: UnOp, e: Box<Expr> },
}

/// Binary operators that are safe to generate (no Div/Mod to avoid div-by-zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    BAnd,
    BOr,
    BXor,
}

/// Unary operators supported by both the Coq semantics and Rust lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnOp {
    Neg,
}

impl BinOp {
    pub fn mpc_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::BAnd => "&",
            BinOp::BOr => "|",
            BinOp::BXor => "^",
        }
    }
    pub fn json_str(self) -> &'static str {
        match self {
            BinOp::Add => "Add",
            BinOp::Sub => "Sub",
            BinOp::Mul => "Mul",
            BinOp::BAnd => "BAnd",
            BinOp::BOr => "BOr",
            BinOp::BXor => "BXor",
        }
    }
}

impl UnOp {
    pub fn mpc_str(self) -> &'static str {
        match self {
            UnOp::Neg => "-",
        }
    }
    pub fn json_str(self) -> &'static str {
        match self {
            UnOp::Neg => "Neg",
        }
    }
}

// ---- Rendering --------------------------------------------------------------

impl Expr {
    /// Render to `.mpc` source, always parenthesized to avoid precedence issues.
    pub fn to_mpc(&self) -> String {
        match self {
            Expr::Var { n } => n.clone(),
            Expr::Const { v } => v.to_string(),
            Expr::BinOp { op, l, r } => {
                format!("({} {} {})", l.to_mpc(), op.mpc_str(), r.to_mpc())
            }
            Expr::UnOp { op, e } => {
                format!("({}{})", op.mpc_str(), e.to_mpc())
            }
        }
    }

    /// Render to JSON value (compatible with the OCaml runner's parser).
    pub fn to_json_value(&self) -> serde_json::Value {
        match self {
            Expr::Var { n } => {
                serde_json::json!({"t": "Var", "n": n})
            }
            Expr::Const { v } => {
                serde_json::json!({"t": "Const", "v": v})
            }
            Expr::BinOp { op, l, r } => {
                serde_json::json!({
                    "t": "BinOp",
                    "op": op.json_str(),
                    "l": l.to_json_value(),
                    "r": r.to_json_value()
                })
            }
            Expr::UnOp { op, e } => {
                serde_json::json!({
                    "t": "UnOp",
                    "op": op.json_str(),
                    "e": e.to_json_value()
                })
            }
        }
    }
}

impl TestCase {
    /// Render the test case as `.mpc` source (a `main` function).
    pub fn to_mpc(&self) -> String {
        let params: String = self
            .params
            .iter()
            .map(|p| format!("Public Field<{}> {}", self.field_size, p))
            .collect::<Vec<_>>()
            .join(", ");

        let mut body = String::new();
        for lb in &self.lets {
            body.push_str(&format!(
                "    let {} : Field<{}> = {};\n",
                lb.name,
                self.field_size,
                lb.expr.to_mpc()
            ));
        }
        body.push_str(&format!("    return {};\n", self.ret.to_mpc()));

        format!(
            "fn main({}) -> Field<{}> {{\n{}}}",
            params, self.field_size, body
        )
    }

    /// Render as JSON for the Coq oracle runner.
    pub fn to_json(&self) -> String {
        let lets: Vec<serde_json::Value> = self
            .lets
            .iter()
            .map(|lb| {
                serde_json::json!({
                    "name": lb.name,
                    "expr": lb.expr.to_json_value()
                })
            })
            .collect();

        let v = serde_json::json!({
            "field_size": self.field_size,
            "params":     self.params,
            "inputs":     self.inputs,
            "lets":       lets,
            "ret":        self.ret.to_json_value()
        });
        serde_json::to_string(&v).unwrap()
    }
}

// ---- Proptest strategies ----------------------------------------------------

fn arb_binop() -> impl Strategy<Value = BinOp> {
    prop_oneof![
        Just(BinOp::Add),
        Just(BinOp::Sub),
        Just(BinOp::Mul),
        Just(BinOp::BAnd),
        Just(BinOp::BOr),
        Just(BinOp::BXor),
    ]
}

fn arb_unop() -> impl Strategy<Value = UnOp> {
    Just(UnOp::Neg)
}

/// Generate an expression over the given `vars` (at most depth 3 deep).
/// Constants are drawn from `0..=max_const`.
pub fn arb_expr(vars: Vec<String>, max_const: u64) -> impl Strategy<Value = Expr> {
    let leaf = {
        let vars2 = vars.clone();
        prop_oneof![
            (0u64..=max_const).prop_map(|v| Expr::Const { v }),
            proptest::sample::select(vars2).prop_map(|n| Expr::Var { n }),
        ]
    };

    leaf.prop_recursive(
        3,  // max depth
        64, // max nodes
        4,  // items per collection
        move |inner| {
            prop_oneof![
                // BinOp: two sub-expressions
                (arb_binop(), inner.clone(), inner.clone()).prop_map(|(op, l, r)| Expr::BinOp {
                    op,
                    l: Box::new(l),
                    r: Box::new(r),
                }),
                // UnOp: one sub-expression
                (arb_unop(), inner).prop_map(|(op, e)| Expr::UnOp {
                    op,
                    e: Box::new(e),
                }),
            ]
        },
    )
}

/// Generate a syntactically valid but possibly type-incorrect source program.
/// Used for crash-safety testing (Layer 2).
pub fn arb_program_source() -> impl Strategy<Value = String> {
    let field_size = prop_oneof![Just(8u32), Just(16u32), Just(32u32), Just(64u32)];
    let n_params = 1usize..=3;
    let n_lets = 0usize..=4;
    // Produce a somewhat-valid but not necessarily correct program
    (field_size, n_params, n_lets, 0u64..100u64).prop_map(
        |(fs, np, nl, c)| {
            let params: String = (0..np)
                .map(|i| format!("Public Field<{}> p{}", fs, i))
                .collect::<Vec<_>>()
                .join(", ");
            let mut body = String::new();
            for i in 0..nl {
                body.push_str(&format!(
                    "    let v{} : Field<{}> = p0 + {};\n",
                    i, fs, c
                ));
            }
            body.push_str("    return p0;\n");
            format!("fn main({}) -> Field<{}> {{\n{}}}", params, fs, body)
        },
    )
}

/// Generate a fully type-correct test case (Layer 3 — semantic differential).
///
/// Strategy:
/// - All parameters and let-bound variables have type `Field<field_size>`
/// - Each let binding expression uses only the function parameters
///   (not other let-bound vars — avoids sequential generation complexity)
/// - The return expression may use params + all let-bound vars
pub fn arb_test_case() -> impl Strategy<Value = TestCase> {
    (
        prop_oneof![Just(64u32)], // field_size; start with 64 only
        1usize..=3usize,          // n_params
        0usize..=5usize,          // n_lets
    )
        .prop_flat_map(|(field_size, n_params, n_lets)| {
            let params: Vec<String> = (0..n_params).map(|i| format!("p{}", i)).collect();
            let let_names: Vec<String> = (0..n_lets).map(|i| format!("v{}", i)).collect();

            // Inputs: field-sized values (small to avoid modular surprises)
            let inputs_strat =
                proptest::collection::vec(0u64..1000, n_params..=n_params);

            // Let expressions: use only params
            let let_exprs_strat: Vec<_> = (0..n_lets)
                .map(|_| arb_expr(params.clone(), 100))
                .collect();

            // Return expression: use params + all let names
            let all_vars: Vec<String> =
                params.iter().cloned().chain(let_names.iter().cloned()).collect();
            let ret_strat = arb_expr(all_vars, 100);

            // params/let_names are fixed for this flat_map invocation
            let params2 = params.clone();
            let let_names2 = let_names.clone();

            (
                Just(field_size),
                Just(params2),
                Just(let_names2),
                inputs_strat,
                let_exprs_strat,
                ret_strat,
            )
                .prop_map(|(fs, ps, lns, inputs, let_exprs, ret)| {
                    let lets = lns
                        .into_iter()
                        .zip(let_exprs)
                        .map(|(name, expr)| LetBinding { name, expr })
                        .collect();
                    TestCase {
                        field_size: fs,
                        params: ps,
                        inputs,
                        lets,
                        ret,
                    }
                })
        })
}

// ---- Tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_simple() {
        let tc = TestCase {
            field_size: 64,
            params: vec!["p0".into(), "p1".into()],
            inputs: vec![5, 3],
            lets: vec![LetBinding {
                name: "v0".into(),
                expr: Expr::BinOp {
                    op: BinOp::Add,
                    l: Box::new(Expr::Var { n: "p0".into() }),
                    r: Box::new(Expr::Var { n: "p1".into() }),
                },
            }],
            ret: Expr::Var { n: "v0".into() },
        };
        let mpc = tc.to_mpc();
        assert!(mpc.contains("fn main("));
        assert!(mpc.contains("Field<64>"));
        assert!(mpc.contains("return v0"));

        let json = tc.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["field_size"], 64);
        assert_eq!(parsed["params"][0], "p0");
    }
}
