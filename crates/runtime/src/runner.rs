use net::NetworkLayer;
use thiserror::Error;

use crate::{
    compiler::compile_to_vm_instructions,
    vm::{Backend, BackendError, Instruction, VMState},
};
use ir::lir::{Program, WireId};

// ---- Error ----

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("backend: {0}")]
    Backend(#[from] BackendError),
    #[error("network: {0}")]
    Network(#[from] anyhow::Error),
}

// ---- Step ----

/// Result of a single [`Runner::next`] call.
pub enum Step {
    /// One instruction was executed (and any resulting communication round was
    /// completed).  Call `next()` again to continue.
    Progress,
    /// All instructions have executed.  Contains the output wire values.
    Done(Vec<(WireId, u64)>),
}

// ---- Runner ----

/// Orchestrates MPC protocol execution for a single party.
///
/// The runner owns both a **network layer** and a **crypto backend**.  On each
/// call to [`next`] it:
/// 1. Executes the next instruction via the backend.
/// 2. Asks the backend for any outgoing messages ([`Backend::take_outgoing`]).
/// 3. Sends those messages to the appropriate peers via the network layer.
/// 4. Receives the corresponding replies.
/// 5. Delivers the replies back to the backend ([`Backend::receive_replies`]).
///
/// This design is fully generic: swap in a different `N` or `B` (including
/// the provided [`net::StubNetwork`] for tests) without touching any other
/// code.
///
/// [`next`]: Runner::next
pub struct Runner<N, B> {
    network: N,
    backend: B,
    program: Program,
    state: VMState,
    instructions: Vec<Instruction>,
    pc: usize,
}

impl<N: NetworkLayer, B: Backend> Runner<N, B> {
    /// Construct a runner, set all inputs, and compile the circuit.
    pub fn new(
        network: N,
        mut backend: B,
        program: Program,
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

        Ok(Self { network, backend, program, state, instructions, pc: 0 })
    }

    /// Execute one instruction and complete any resulting communication round.
    ///
    /// Returns [`Step::Progress`] until all instructions are done, then
    /// [`Step::Done`] with the output values.
    pub async fn next(&mut self) -> Result<Step, RunnerError> {
        if self.pc >= self.instructions.len() {
            let mut outputs = Vec::new();
            for wire in &self.program.circuit.outputs {
                let value = self.backend.get_output(*wire, &self.state)?;
                outputs.push((*wire, value));
            }
            return Ok(Step::Done(outputs));
        }

        // Execute one instruction.
        let instr = self.instructions[self.pc].clone();
        self.backend.execute_instruction(&instr, &mut self.state)?;
        self.pc += 1;

        // If the backend needs a communication round, handle it now.
        let outgoing = self.backend.take_outgoing();
        if !outgoing.is_empty() {
            // Determine which peers are involved (the ones we're sending to).
            let peers: Vec<usize> = outgoing.iter().map(|(id, _)| *id).collect();

            // Send all outgoing messages first to avoid deadlock.
            for (to, msg) in outgoing {
                self.network.send_to(to, msg).await?;
            }

            // Receive the corresponding reply from each peer.
            let mut replies = Vec::with_capacity(peers.len());
            for from in peers {
                let msg = self.network.recv_from(from).await?;
                replies.push((from, msg));
            }

            self.backend.receive_replies(replies)?;
        }

        Ok(Step::Progress)
    }

    /// Run the circuit to completion and return all output values.
    pub async fn run(&mut self) -> Result<Vec<(WireId, u64)>, RunnerError> {
        loop {
            match self.next().await? {
                Step::Progress => {}
                Step::Done(outputs) => return Ok(outputs),
            }
        }
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use ir::lir::{CircuitBuilder, GateType, Metadata, PartyId, Program, Statistics, Visibility, WireId};
    use net::stub_networks;

    use crate::ClearBackend;

    /// Build a trivial program: one Add gate, two public inputs, one output.
    fn add_program() -> Program {
        let mut b = CircuitBuilder::new();
        let w0 = b.add_input(Visibility::Public, None);
        let w1 = b.add_input(Visibility::Public, None);
        let out = b.add_gate(GateType::Add, vec![w0, w1]);
        b.add_output(out);
        let metadata = Metadata {
            version: "test".into(),
            source_file: "test".into(),
            function_name: "add".into(),
            field_modulus: None,
            statistics: Statistics {
                total_gates: 1,
                gate_counts: Default::default(),
                circuit_depth: 1,
                num_inputs: 2,
                num_outputs: 1,
                num_wires: 3,
            },
        };
        b.build(metadata)
    }

    /// Single-party runner with StubNetwork + ClearBackend.
    #[tokio::test]
    async fn single_party_clear_add() {
        let mut stubs = stub_networks(1);
        let net = stubs.remove(0);
        let program = add_program();
        let inputs = vec![
            (WireId(0), PartyId(0), 3u64),
            (WireId(1), PartyId(0), 5u64),
        ];

        let mut runner = Runner::new(net, ClearBackend::new(None), program, &inputs).unwrap();
        let outputs = runner.run().await.unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].1, 8);
    }

    /// Two-party runner: both parties run the same program concurrently with a
    /// StubNetwork.  ClearBackend has no comms, so this validates the generic
    /// runner API without requiring real MPC communication.
    #[tokio::test]
    async fn two_party_concurrent_clear() {
        let mut stubs = stub_networks(2);
        let net0 = stubs.remove(0);
        let net1 = stubs.remove(0);
        let program = add_program();

        let inputs0 = vec![
            (WireId(0), PartyId(0), 10u64),
            (WireId(1), PartyId(0), 20u64),
        ];
        let inputs1 = inputs0.clone();
        let program1 = program.clone();

        let t0 = tokio::spawn(async move {
            let mut r = Runner::new(net0, ClearBackend::new(None), program, &inputs0).unwrap();
            r.run().await.unwrap()
        });
        let t1 = tokio::spawn(async move {
            let mut r = Runner::new(net1, ClearBackend::new(None), program1, &inputs1).unwrap();
            r.run().await.unwrap()
        });

        assert_eq!(t0.await.unwrap()[0].1, 30);
        assert_eq!(t1.await.unwrap()[0].1, 30);
    }
}
