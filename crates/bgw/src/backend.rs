use crate::field::{field_to_u64_checked, u64_to_field};
use crate::ir::{BgwNodeId, BgwOp, BgwProgram};
use crate::lowering::lower_instruction;
use crate::ops::{add_shares, multiply_shares, sub_shares};
use crate::shamir::{reconstruct_secret, share_secret};
use crate::types::{PartyShares, Share};
use ark_bls12_381::Fr;
use ir::lir::WireId;
use runtime::vm::{Backend, BackendError, Instruction, VMState, WireValue};
use runtime::Visibility;
use std::collections::HashMap;

const DEFAULT_RNG_SEED: u64 = 0x_4247_575f_5345_4544;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BgwConfig {
    pub parties: usize,
    pub threshold: usize,
}

#[derive(Debug)]
pub struct BgwBackend {
    config: BgwConfig,
    program: BgwProgram,
    wire_shares: HashMap<WireId, PartyShares<Fr>>,
    node_values: HashMap<BgwNodeId, PartyShares<Fr>>,
    output_cache: HashMap<WireId, u64>,
    executed: bool,
    rng: ark_std::rand::rngs::StdRng,
}

impl BgwBackend {
    pub fn new(config: BgwConfig) -> Result<Self, BackendError> {
        if config.parties == 0 {
            return Err(BackendError::BackendError(
                "Invalid BGW config: parties must be > 0".to_string(),
            ));
        }
        if config.threshold == 0 {
            return Err(BackendError::BackendError(
                "Invalid BGW config: threshold must be > 0".to_string(),
            ));
        }
        if config.threshold > config.parties {
            return Err(BackendError::BackendError(
                "Invalid BGW config: threshold must be <= parties".to_string(),
            ));
        }

        use ark_std::rand::SeedableRng;
        Ok(Self {
            config,
            program: BgwProgram::default(),
            wire_shares: HashMap::new(),
            node_values: HashMap::new(),
            output_cache: HashMap::new(),
            executed: false,
            rng: ark_std::rand::rngs::StdRng::seed_from_u64(DEFAULT_RNG_SEED),
        })
    }

    fn arithmetic_error<E: core::fmt::Debug>(context: &str, err: E) -> BackendError {
        BackendError::ArithmeticError(format!("BGW {} failed: {:?}", context, err))
    }

    fn constant_shares(&self, value: Fr) -> PartyShares<Fr> {
        let shares: Vec<Share<Fr>> = (0..self.config.parties)
            .map(|_| Share(value))
            .collect();
        PartyShares::new(shares)
    }

    fn execute_program_once(&mut self) -> Result<(), BackendError> {
        if self.executed {
            return Ok(());
        }

        for (idx, op) in self.program.nodes.iter().enumerate() {
            let node_id = BgwNodeId(idx);
            let value = match *op {
                BgwOp::Input { wire } => self
                    .wire_shares
                    .get(&wire)
                    .cloned()
                    .ok_or(BackendError::WireNotSet(wire))?,
                BgwOp::Const { value } => self.constant_shares(value),
                BgwOp::Add { a, b } => {
                    let av = self.node_values.get(&a).ok_or_else(|| {
                        BackendError::BackendError(format!("Missing node value {:?}", a))
                    })?;
                    let bv = self.node_values.get(&b).ok_or_else(|| {
                        BackendError::BackendError(format!("Missing node value {:?}", b))
                    })?;
                    add_shares(av.as_slice(), bv.as_slice())
                        .map_err(|e| Self::arithmetic_error("add", e))?
                }
                BgwOp::Sub { a, b } => {
                    let av = self.node_values.get(&a).ok_or_else(|| {
                        BackendError::BackendError(format!("Missing node value {:?}", a))
                    })?;
                    let bv = self.node_values.get(&b).ok_or_else(|| {
                        BackendError::BackendError(format!("Missing node value {:?}", b))
                    })?;
                    sub_shares(av.as_slice(), bv.as_slice())
                        .map_err(|e| Self::arithmetic_error("sub", e))?
                }
                BgwOp::Mul { a, b } => {
                    let av = self.node_values.get(&a).ok_or_else(|| {
                        BackendError::BackendError(format!("Missing node value {:?}", a))
                    })?;
                    let bv = self.node_values.get(&b).ok_or_else(|| {
                        BackendError::BackendError(format!("Missing node value {:?}", b))
                    })?;
                    multiply_shares(
                        av.as_slice(),
                        bv.as_slice(),
                        self.config.threshold,
                        &mut self.rng,
                    )
                    .map_err(|e| Self::arithmetic_error("mul", e))?
                }
            };
            self.node_values.insert(node_id, value);
        }

        for (wire, node) in &self.program.wire_to_node {
            if let Some(value) = self.node_values.get(node) {
                self.wire_shares.insert(*wire, value.clone());
            }
        }

        self.executed = true;
        Ok(())
    }
}

impl Backend for BgwBackend {
    fn execute_instruction(
        &mut self,
        instruction: &Instruction,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        self.executed = false;
        lower_instruction(&mut self.program, instruction)?;

        match instruction {
            Instruction::And { vis, output, .. }
            | Instruction::Xor { vis, output, .. } => {
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
            }
            Instruction::Not { vis, output, .. } => {
                state.set_wire(*output, WireValue::Secret, *vis);
            }
            Instruction::Add { vis, output, .. }
            | Instruction::Mul { vis, output, .. }
            | Instruction::Sub { vis, output, .. } => {
                state.set_wire(*output, WireValue::Secret, vis.output_visibility());
            }
            Instruction::Constant {
                output, visibility, ..
            } => {
                state.set_wire(*output, WireValue::Secret, *visibility);
            }
            Instruction::AddConstant { vis, output, .. }
            | Instruction::MulConstant { vis, output, .. }
            | Instruction::SubConstant { vis, output, .. } => {
                state.set_wire(*output, WireValue::Secret, *vis);
            }
            Instruction::Div { .. } | Instruction::Mod { .. } => {}
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "BGW Arithmetic"
    }

    fn set_input(
        &mut self,
        wire: WireId,
        value: u64,
        visibility: Visibility,
        state: &mut VMState,
    ) -> Result<(), BackendError> {
        let shares = share_secret(
            u64_to_field::<Fr>(value),
            self.config.threshold,
            self.config.parties,
            &mut self.rng,
        )
        .map_err(|e| Self::arithmetic_error("share input", e))?;

        self.wire_shares.insert(wire, shares);
        state.set_wire(wire, WireValue::Secret, visibility);
        Ok(())
    }

    fn get_output(&mut self, wire: WireId, _state: &VMState) -> Result<u64, BackendError> {
        if let Some(cached) = self.output_cache.get(&wire).copied() {
            return Ok(cached);
        }

        self.execute_program_once()?;
        let shares = self.wire_shares.get(&wire).ok_or(BackendError::WireNotSet(wire))?;
        let secret = reconstruct_secret(shares.as_slice())
            .map_err(|e| Self::arithmetic_error("reconstruct", e))?;
        let value = field_to_u64_checked(secret)
            .map_err(|e| Self::arithmetic_error("u64 conversion", e))?;
        self.output_cache.insert(wire, value);
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::lir::{Gate, GateId, GateType, Input, Metadata, PartyId, Program, Statistics};
    use runtime::executor::execute_program;

    fn test_metadata() -> Metadata {
        Metadata {
            version: "1.0".to_string(),
            source_file: "bgw_backend_test".to_string(),
            function_name: "bgw_backend_test".to_string(),
            field_modulus: None,
            statistics: Statistics {
                total_gates: 0,
                gate_counts: HashMap::new(),
                circuit_depth: 0,
                num_inputs: 0,
                num_outputs: 0,
                num_wires: 0,
            },
        }
    }

    fn mk_program(inputs: usize, gates: Vec<Gate>, outputs: Vec<WireId>) -> Program {
        Program {
            metadata: test_metadata(),
            circuit: ir::lir::Circuit {
                inputs: (0..inputs)
                    .map(|i| Input {
                        wire: WireId(i),
                        party: None,
                        visibility: Visibility::Secret,
                        name: Some(format!("in{}", i)),
                    })
                    .collect(),
                gates,
                outputs,
            },
        }
    }

    #[test]
    fn backend_accepts_inputs_and_runs_lowered_arithmetic_program() {
        let program = mk_program(
            3,
            vec![
                Gate {
                    id: GateId(0),
                    gate_type: GateType::Add,
                    inputs: vec![WireId(0), WireId(1)],
                    output: WireId(3),
                },
                Gate {
                    id: GateId(1),
                    gate_type: GateType::Mul,
                    inputs: vec![WireId(3), WireId(2)],
                    output: WireId(4),
                },
                Gate {
                    id: GateId(2),
                    gate_type: GateType::Sub,
                    inputs: vec![WireId(4), WireId(1)],
                    output: WireId(5),
                },
            ],
            vec![WireId(5)],
        );

        let mut backend = BgwBackend::new(BgwConfig {
            parties: 5,
            threshold: 3,
        })
        .unwrap();

        let out = execute_program(
            &program,
            &mut backend,
            &[
                (WireId(0), PartyId(0), 7),
                (WireId(1), PartyId(1), 5),
                (WireId(2), PartyId(2), 9),
            ],
        )
        .unwrap();

        assert_eq!(out[0], (WireId(5), (7 + 5) * 9 - 5));
    }

    #[test]
    fn backend_runs_boolean_mapped_program() {
        let program = mk_program(
            2,
            vec![
                Gate {
                    id: GateId(0),
                    gate_type: GateType::Xor,
                    inputs: vec![WireId(0), WireId(1)],
                    output: WireId(2),
                },
                Gate {
                    id: GateId(1),
                    gate_type: GateType::Not,
                    inputs: vec![WireId(2)],
                    output: WireId(3),
                },
                Gate {
                    id: GateId(2),
                    gate_type: GateType::And,
                    inputs: vec![WireId(0), WireId(3)],
                    output: WireId(4),
                },
            ],
            vec![WireId(4)],
        );

        let mut backend = BgwBackend::new(BgwConfig {
            parties: 5,
            threshold: 3,
        })
        .unwrap();

        let out = execute_program(
            &program,
            &mut backend,
            &[(WireId(0), PartyId(0), 1), (WireId(1), PartyId(1), 0)],
        )
        .unwrap();

        assert_eq!(out[0], (WireId(4), 0));
    }

    #[test]
    fn backend_invalid_config_errors() {
        let err = BgwBackend::new(BgwConfig {
            parties: 2,
            threshold: 3,
        })
        .unwrap_err();
        assert!(format!("{}", err).contains("threshold"));
    }
}
