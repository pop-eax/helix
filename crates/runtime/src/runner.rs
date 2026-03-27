use crate::{
    compiler::compile_to_vm_instructions,
    vm::{Backend, BackendError, Instruction, VMState},
};
use ir::lir::{Program, WireId};

pub enum Step {
    /// Messages that must be exchanged before calling `next()` again.
    /// Each entry is `(party_id, payload)`.
    NeedsComm(Vec<(usize, Vec<u8>)>),
    /// Execution is complete. Contains the output wire values.
    Done(Vec<(WireId, u64)>),
}

pub struct Runner<B: Backend> {
    program: Program,
    backend: B,
    state: VMState,
    instructions: Vec<Instruction>,
    pc: usize,
}

impl<B: Backend> Runner<B> {
    pub fn new(
        program: Program,
        mut backend: B,
        inputs: &[(WireId, ir::lir::PartyId, u64)],
    ) -> Result<Self, BackendError> {
        let num_wires = program
            .circuit
            .gates
            .iter()
            .map(|g| g.output.0 as usize)
            .chain(program.circuit.inputs.iter().map(|i| i.wire.0 as usize))
            .max()
            .unwrap_or(0)
            + 1;

        let mut state = VMState::new(
            num_wires,
            program.metadata.field_modulus.unwrap_or(2_u64.pow(63) - 1),
        );

        for (wire, _party, value) in inputs {
            let visibility = program
                .circuit
                .inputs
                .iter()
                .find(|i| i.wire == *wire)
                .map(|i| i.visibility)
                .unwrap_or(ir::lir::Visibility::Public);

            backend.set_input(*wire, *value, visibility, &mut state)?;
        }

        let instructions = compile_to_vm_instructions(&program.circuit);

        Ok(Self { program, backend, state, instructions, pc: 0 })
    }

    /// Advance execution. Returns `Step::NeedsComm` if the backend needs a
    /// communication round, or `Step::Done` when all instructions have run.
    pub fn next(&mut self) -> Result<Step, BackendError> {
        loop {
            if self.pc >= self.instructions.len() {
                let mut outputs = Vec::new();
                for wire in &self.program.circuit.outputs {
                    let value = self.backend.get_output(*wire, &self.state)?;
                    outputs.push((*wire, value));
                }
                return Ok(Step::Done(outputs));
            }

            let instr = self.instructions[self.pc].clone();
            self.backend.execute_instruction(&instr, &mut self.state)?;
            self.pc += 1;

            let outgoing = self.backend.take_outgoing();
            if !outgoing.is_empty() {
                return Ok(Step::NeedsComm(outgoing));
            }
        }
    }

    /// Deliver replies from other parties after a `Step::NeedsComm`.
    pub fn feed_replies(&mut self, messages: Vec<(usize, Vec<u8>)>) -> Result<(), BackendError> {
        self.backend.receive_replies(messages)
    }
}
