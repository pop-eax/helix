use crate::field::u64_to_field;
use crate::ir::{BgwNodeId, BgwOp, BgwProgram};
use ark_bls12_381::Fr;
use ir::lir::WireId;
use runtime::vm::{BackendError, Instruction};

pub fn ensure_input_node(program: &mut BgwProgram, wire: WireId) -> BgwNodeId {
    if let Some(node) = program.get_wire_node(wire) {
        return node;
    }
    let node = program.push_node(BgwOp::Input { wire });
    program.set_wire_node(wire, node);
    node
}

pub fn lower_constant(program: &mut BgwProgram, output: WireId, value: u64) -> BgwNodeId {
    let node = program.push_node(BgwOp::Const {
        value: u64_to_field::<Fr>(value),
    });
    program.set_wire_node(output, node);
    node
}

fn get_wire_node(program: &BgwProgram, wire: WireId) -> Result<BgwNodeId, BackendError> {
    program.get_wire_node(wire).ok_or(BackendError::WireNotSet(wire))
}

fn lower_add(
    program: &mut BgwProgram,
    input1: WireId,
    input2: WireId,
    output: WireId,
) -> Result<(), BackendError> {
    ensure_input_node(program, input1);
    ensure_input_node(program, input2);
    let a = get_wire_node(program, input1)?;
    let b = get_wire_node(program, input2)?;
    let out = program.push_node(BgwOp::Add { a, b });
    program.set_wire_node(output, out);
    Ok(())
}

fn lower_sub(
    program: &mut BgwProgram,
    input1: WireId,
    input2: WireId,
    output: WireId,
) -> Result<(), BackendError> {
    ensure_input_node(program, input1);
    ensure_input_node(program, input2);
    let a = get_wire_node(program, input1)?;
    let b = get_wire_node(program, input2)?;
    let out = program.push_node(BgwOp::Sub { a, b });
    program.set_wire_node(output, out);
    Ok(())
}

fn lower_mul(
    program: &mut BgwProgram,
    input1: WireId,
    input2: WireId,
    output: WireId,
) -> Result<(), BackendError> {
    ensure_input_node(program, input1);
    ensure_input_node(program, input2);
    let a = get_wire_node(program, input1)?;
    let b = get_wire_node(program, input2)?;
    let out = program.push_node(BgwOp::Mul { a, b });
    program.set_wire_node(output, out);
    Ok(())
}

fn lower_not(program: &mut BgwProgram, input: WireId, output: WireId) -> Result<(), BackendError> {
    ensure_input_node(program, input);
    let one = program.push_node(BgwOp::Const {
        value: u64_to_field::<Fr>(1),
    });
    let x = get_wire_node(program, input)?;
    let out = program.push_node(BgwOp::Sub { a: one, b: x });
    program.set_wire_node(output, out);
    Ok(())
}

fn lower_and(
    program: &mut BgwProgram,
    input1: WireId,
    input2: WireId,
    output: WireId,
) -> Result<(), BackendError> {
    lower_mul(program, input1, input2, output)
}

fn lower_xor(
    program: &mut BgwProgram,
    input1: WireId,
    input2: WireId,
    output: WireId,
) -> Result<(), BackendError> {
    ensure_input_node(program, input1);
    ensure_input_node(program, input2);
    let a = get_wire_node(program, input1)?;
    let b = get_wire_node(program, input2)?;
    let add = program.push_node(BgwOp::Add { a, b });
    let mul = program.push_node(BgwOp::Mul { a, b });
    let two = program.push_node(BgwOp::Const {
        value: u64_to_field::<Fr>(2),
    });
    let two_mul = program.push_node(BgwOp::Mul { a: two, b: mul });
    let out = program.push_node(BgwOp::Sub {
        a: add,
        b: two_mul,
    });
    program.set_wire_node(output, out);
    Ok(())
}

fn lower_or(
    program: &mut BgwProgram,
    input1: WireId,
    input2: WireId,
    output: WireId,
) -> Result<(), BackendError> {
    // OR(a,b) = a + b - a*b  (correct for boolean inputs 0/1)
    ensure_input_node(program, input1);
    ensure_input_node(program, input2);
    let a = get_wire_node(program, input1)?;
    let b = get_wire_node(program, input2)?;
    let ab = program.push_node(BgwOp::Mul { a, b });
    let apb = program.push_node(BgwOp::Add { a, b });
    let out = program.push_node(BgwOp::Sub { a: apb, b: ab });
    program.set_wire_node(output, out);
    Ok(())
}

fn lower_add_constant(
    program: &mut BgwProgram,
    input: WireId,
    constant: u64,
    output: WireId,
) -> Result<(), BackendError> {
    ensure_input_node(program, input);
    let x = get_wire_node(program, input)?;
    let c = program.push_node(BgwOp::Const {
        value: u64_to_field::<Fr>(constant),
    });
    let out = program.push_node(BgwOp::Add { a: x, b: c });
    program.set_wire_node(output, out);
    Ok(())
}

fn lower_sub_constant(
    program: &mut BgwProgram,
    input: WireId,
    constant: u64,
    output: WireId,
) -> Result<(), BackendError> {
    ensure_input_node(program, input);
    let x = get_wire_node(program, input)?;
    let c = program.push_node(BgwOp::Const {
        value: u64_to_field::<Fr>(constant),
    });
    let out = program.push_node(BgwOp::Sub { a: x, b: c });
    program.set_wire_node(output, out);
    Ok(())
}

fn lower_mul_constant(
    program: &mut BgwProgram,
    input: WireId,
    constant: u64,
    output: WireId,
) -> Result<(), BackendError> {
    ensure_input_node(program, input);
    let x = get_wire_node(program, input)?;
    let c = program.push_node(BgwOp::Const {
        value: u64_to_field::<Fr>(constant),
    });
    let out = program.push_node(BgwOp::Mul { a: x, b: c });
    program.set_wire_node(output, out);
    Ok(())
}

pub fn lower_instruction(
    program: &mut BgwProgram,
    instruction: &Instruction,
) -> Result<(), BackendError> {
    match instruction {
        Instruction::And {
            input1,
            input2,
            output,
            ..
        } => lower_and(program, *input1, *input2, *output),
        Instruction::Xor {
            input1,
            input2,
            output,
            ..
        } => lower_xor(program, *input1, *input2, *output),
        Instruction::Or {
            input1,
            input2,
            output,
            ..
        } => lower_or(program, *input1, *input2, *output),
        Instruction::Not { input, output, .. } => lower_not(program, *input, *output),
        Instruction::Add {
            input1,
            input2,
            output,
            ..
        } => lower_add(program, *input1, *input2, *output),
        Instruction::Sub {
            input1,
            input2,
            output,
            ..
        } => lower_sub(program, *input1, *input2, *output),
        Instruction::Mul {
            input1,
            input2,
            output,
            ..
        } => lower_mul(program, *input1, *input2, *output),
        Instruction::Constant { value, output, .. } => {
            lower_constant(program, *output, *value);
            Ok(())
        }
        Instruction::AddConstant {
            input,
            constant,
            output,
            ..
        } => lower_add_constant(program, *input, *constant, *output),
        Instruction::MulConstant {
            input,
            constant,
            output,
            ..
        } => lower_mul_constant(program, *input, *constant, *output),
        Instruction::SubConstant {
            input,
            constant,
            output,
            ..
        } => lower_sub_constant(program, *input, *constant, *output),
        Instruction::Div { .. } => Err(BackendError::BackendError(
            "Instruction Div is unsupported in BGW backend v1".to_string(),
        )),
        Instruction::Mod { .. } => Err(BackendError::BackendError(
            "Instruction Mod is unsupported in BGW backend v1".to_string(),
        )),
        Instruction::LessThan { .. } | Instruction::Equal { .. } => Err(BackendError::BackendError(
            "comparison gates require the Yao backend (garbled circuits); \
             not supported in arithmetic BGW MPC"
                .to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::lir::Visibility;
    use runtime::vm::VisibilityPair;

    #[test]
    fn lower_add_emits_add_node_and_wire_binding() {
        let mut p = BgwProgram::default();
        lower_instruction(
            &mut p,
            &Instruction::Add {
                vis: VisibilityPair::new(Visibility::Secret, Visibility::Secret),
                input1: WireId(1),
                input2: WireId(2),
                output: WireId(3),
                field_size: 64,
            },
        )
        .unwrap();

        assert!(matches!(p.nodes[0], BgwOp::Input { wire } if wire == WireId(1)));
        assert!(matches!(p.nodes[1], BgwOp::Input { wire } if wire == WireId(2)));
        assert!(matches!(p.nodes[2], BgwOp::Add { .. }));
        assert_eq!(p.get_wire_node(WireId(3)), Some(BgwNodeId(2)));
    }

    #[test]
    fn lower_constant_ops_emit_const_plus_op_nodes() {
        let mut p = BgwProgram::default();
        ensure_input_node(&mut p, WireId(0));

        lower_instruction(
            &mut p,
            &Instruction::AddConstant {
                vis: Visibility::Secret,
                input: WireId(0),
                constant: 7,
                output: WireId(1),
                field_size: 64,
            },
        )
        .unwrap();

        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Const { .. })));
        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Add { .. })));
        assert!(p.get_wire_node(WireId(1)).is_some());
    }

    #[test]
    fn lower_not_maps_to_sub_of_one() {
        let mut p = BgwProgram::default();
        lower_instruction(
            &mut p,
            &Instruction::Not {
                vis: Visibility::Secret,
                input: WireId(1),
                output: WireId(2),
            },
        )
        .unwrap();
        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Sub { .. })));
        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Const { .. })));
    }

    #[test]
    fn lower_and_maps_to_mul() {
        let mut p = BgwProgram::default();
        lower_instruction(
            &mut p,
            &Instruction::And {
                vis: VisibilityPair::new(Visibility::Secret, Visibility::Secret),
                input1: WireId(0),
                input2: WireId(1),
                output: WireId(2),
            },
        )
        .unwrap();
        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Mul { .. })));
    }

    #[test]
    fn lower_xor_maps_to_add_sub_with_2ab() {
        let mut p = BgwProgram::default();
        lower_instruction(
            &mut p,
            &Instruction::Xor {
                vis: VisibilityPair::new(Visibility::Secret, Visibility::Secret),
                input1: WireId(0),
                input2: WireId(1),
                output: WireId(2),
            },
        )
        .unwrap();
        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Add { .. })));
        assert!(p.nodes.iter().filter(|op| matches!(op, BgwOp::Mul { .. })).count() >= 2);
        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Sub { .. })));
    }

    #[test]
    fn lower_or_maps_to_add_sub_mul() {
        let mut p = BgwProgram::default();
        lower_instruction(
            &mut p,
            &Instruction::Or {
                vis: VisibilityPair::new(Visibility::Secret, Visibility::Secret),
                input1: WireId(0),
                input2: WireId(1),
                output: WireId(2),
            },
        )
        .unwrap();
        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Mul { .. })));
        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Add { .. })));
        assert!(p.nodes.iter().any(|op| matches!(op, BgwOp::Sub { .. })));
        assert!(p.get_wire_node(WireId(2)).is_some());
    }

    #[test]
    fn lower_div_returns_unsupported_error() {
        let mut p = BgwProgram::default();
        let err = lower_instruction(
            &mut p,
            &Instruction::Div {
                vis: VisibilityPair::new(Visibility::Secret, Visibility::Secret),
                input1: WireId(0),
                input2: WireId(1),
                output: WireId(2),
                field_size: 64,
            },
        )
        .unwrap_err();
        assert!(format!("{}", err).contains("unsupported"));
    }

    #[test]
    fn lower_mod_returns_unsupported_error() {
        let mut p = BgwProgram::default();
        let err = lower_instruction(
            &mut p,
            &Instruction::Mod {
                vis: VisibilityPair::new(Visibility::Secret, Visibility::Secret),
                input1: WireId(0),
                input2: WireId(1),
                output: WireId(2),
                field_size: 64,
            },
        )
        .unwrap_err();
        assert!(format!("{}", err).contains("unsupported"));
    }
}
