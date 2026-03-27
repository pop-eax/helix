use std::collections::HashMap;

use net::NetworkLayer;
use thiserror::Error;

use crate::{
    compiler::compile_to_vm_instructions,
    vm::{Backend, BackendError, Instruction, VMState},
};
use ir::lir::{Program, Visibility, WireId};

// ---- Error ----

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("backend: {0}")]
    Backend(#[from] BackendError),
    #[error("network: {0}")]
    Network(#[from] anyhow::Error),
}

// ---- Public step type ----

/// Result of a single [`Runner::next`] call.
pub enum Step {
    /// One gate was executed (plus any network communication it required).
    /// Call `next()` again to continue.
    Progress,
    /// All gates have completed.  Contains the output wire values.
    Done(Vec<(WireId, u64)>),
}

// ---- Input assignment ----

/// Describes one input wire and who owns its secret value.
pub struct InputAssignment {
    pub wire: WireId,
    /// Party ID of the input's owner (the one who knows the plaintext).
    pub owner: usize,
    /// The plaintext value — supply `Some(v)` only when `owner == my_id`.
    pub value: Option<u64>,
}

// ---- Private init descriptor ----

struct InitSpec {
    wire: WireId,
    owner: usize,
    visibility: Visibility,
}

// ---- Runner ----

/// Orchestrates MPC protocol execution for a single party.
///
/// # Protocol phases
///
/// 1. **Input distribution** (one-time setup, handled by [`Runner::run`]) —
///    Two passes:
///    - *Send pass*: for every input this party owns, produce per-party shares
///      ([`Backend::share_input`]) and push each share to its destination.
///      Non-owners receive shares; they do not request them.
///    - *Receive pass*: for every input owned by another party, collect the
///      share that was pushed to us.
///
/// 2. **Gate evaluation** — Each gate is executed by the backend step-by-step
///    via [`Runner::next`].  If the backend signals pending communication
///    ([`Backend::take_outgoing`]) the runner routes messages and delivers
///    replies before advancing.
///
/// # Swapping components
///
/// Both `N` and `B` are generic.  Use [`net::StubNetwork`] + [`ClearBackend`]
/// for local tests; swap in [`net::Network`] + a crypto backend for real runs.
///
/// [`ClearBackend`]: crate::ClearBackend
pub struct Runner<N, B> {
    network: N,
    backend: B,
    program: Program,
    state: VMState,
    /// One descriptor per circuit input, in declaration order.
    init_specs: Vec<InitSpec>,
    /// Compiled gate instructions.
    instructions: Vec<Instruction>,
    /// Gate program counter.
    pc: usize,
    /// Set after `init_inputs` completes.
    inputs_done: bool,
    /// Plaintext values for wires owned by this party.
    my_values: HashMap<WireId, u64>,
}

impl<N: NetworkLayer, B: Backend> Runner<N, B> {
    /// Construct a runner for `program`.
    ///
    /// `inputs` must contain one [`InputAssignment`] per circuit input wire.
    /// Set `value` to `Some(plaintext)` for wires this party owns; leave it
    /// `None` for wires owned by others.
    pub fn new(
        network: N,
        backend: B,
        program: Program,
        inputs: &[InputAssignment],
    ) -> Result<Self, RunnerError> {
        let num_wires = program
            .circuit
            .gates
            .iter()
            .map(|g| g.output.0 as usize)
            .chain(program.circuit.inputs.iter().map(|i| i.wire.0 as usize))
            .max()
            .unwrap_or(0)
            + 1;

        let state = VMState::new(
            num_wires,
            program.metadata.field_modulus.unwrap_or(2_u64.pow(63) - 1),
        );

        let assignment_map: HashMap<WireId, &InputAssignment> =
            inputs.iter().map(|a| (a.wire, a)).collect();

        let init_specs = program
            .circuit
            .inputs
            .iter()
            .map(|inp| {
                let a = assignment_map.get(&inp.wire).ok_or_else(|| {
                    RunnerError::Backend(BackendError::BackendError(format!(
                        "no InputAssignment for wire {:?}",
                        inp.wire
                    )))
                })?;
                Ok(InitSpec { wire: inp.wire, owner: a.owner, visibility: inp.visibility })
            })
            .collect::<Result<Vec<_>, RunnerError>>()?;

        let instructions = compile_to_vm_instructions(&program.circuit);

        let my_values = inputs
            .iter()
            .filter_map(|a| a.value.map(|v| (a.wire, v)))
            .collect();

        Ok(Self {
            network,
            backend,
            program,
            state,
            init_specs,
            instructions,
            pc: 0,
            inputs_done: false,
            my_values,
        })
    }

    /// Execute one gate (plus any communication it triggers).
    ///
    /// On the very first call, input distribution runs automatically before
    /// the first gate.  Returns [`Step::Done`] once all gates have run.
    pub async fn next(&mut self) -> Result<Step, RunnerError> {
        if !self.inputs_done {
            self.init_inputs().await?;
            self.inputs_done = true;
        }

        if self.pc >= self.instructions.len() {
            return Ok(Step::Done(self.collect_outputs()?));
        }

        let instr = self.instructions[self.pc].clone();
        self.pc += 1;

        self.backend.execute_instruction(&instr, &mut self.state)?;

        let outgoing = self.backend.take_outgoing();
        if !outgoing.is_empty() {
            let peers: Vec<usize> = outgoing.iter().map(|(id, _)| *id).collect();
            for (to, msg) in outgoing {
                self.network.send_to(to, msg).await?;
            }
            let mut replies = Vec::with_capacity(peers.len());
            for from in peers {
                replies.push((from, self.network.recv_from(from).await?));
            }
            self.backend.receive_replies(replies)?;
        }

        Ok(Step::Progress)
    }

    /// Run the full protocol (input distribution + all gates) to completion.
    pub async fn run(&mut self) -> Result<Vec<(WireId, u64)>, RunnerError> {
        loop {
            match self.next().await? {
                Step::Progress => {}
                Step::Done(outputs) => return Ok(outputs),
            }
        }
    }

    // ---- private ----

    /// Two-pass input distribution.
    ///
    /// **Pass 1 — push owned shares.**  For every input this party owns,
    /// produce per-party shares and send each one to its destination.
    /// Our own share is stored locally immediately.
    ///
    /// **Pass 2 — collect received shares.**  For every input owned by
    /// another party, read the share that was pushed to us.
    ///
    /// Running the sends before the receives avoids deadlock regardless of
    /// whether the network buffers are bounded or unbounded.
    async fn init_inputs(&mut self) -> Result<(), RunnerError> {
        let my_id = self.network.my_id();
        let n = self.network.n_parties();

        // Pass 1: owners push their shares.
        for spec in &self.init_specs {
            if spec.owner != my_id {
                continue;
            }
            let value = self.my_values.get(&spec.wire).copied().ok_or_else(|| {
                RunnerError::Backend(BackendError::BackendError(format!(
                    "party {my_id} owns wire {:?} but no value was provided",
                    spec.wire
                )))
            })?;

            let all_shares = self.backend.share_input(spec.wire, value, n)?;

            // Store our own share.
            self.backend.receive_input_share(
                spec.wire,
                spec.visibility,
                all_shares[my_id].clone(),
                &mut self.state,
            )?;

            // Send every other party their share.
            for (to, share) in all_shares.into_iter().enumerate() {
                if to != my_id {
                    self.network.send_to(to, share).await?;
                }
            }
        }

        // Pass 2: non-owners collect what was pushed to them.
        for spec in &self.init_specs {
            if spec.owner == my_id {
                continue;
            }
            let share = self.network.recv_from(spec.owner).await?;
            self.backend.receive_input_share(
                spec.wire,
                spec.visibility,
                share,
                &mut self.state,
            )?;
        }

        Ok(())
    }

    fn collect_outputs(&mut self) -> Result<Vec<(WireId, u64)>, RunnerError> {
        let mut out = Vec::new();
        for &wire in &self.program.circuit.outputs {
            out.push((wire, self.backend.get_output(wire, &self.state)?));
        }
        Ok(out)
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use ir::lir::{CircuitBuilder, GateType, Metadata, Statistics};
    use net::stub_networks;

    use crate::ClearBackend;

    fn add_program() -> Program {
        let mut b = CircuitBuilder::new();
        let w0 = b.add_input(Visibility::Public, None);
        let w1 = b.add_input(Visibility::Public, None);
        let out = b.add_gate(GateType::Add, vec![w0, w1]);
        b.add_output(out);
        b.build(Metadata {
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
        })
    }

    #[tokio::test]
    async fn single_party_clear_add() {
        let program = add_program();
        let inputs = [
            InputAssignment { wire: WireId(0), owner: 0, value: Some(3) },
            InputAssignment { wire: WireId(1), owner: 0, value: Some(5) },
        ];
        let mut runner =
            Runner::new(stub_networks(1).remove(0), ClearBackend::new(None), program, &inputs)
                .unwrap();
        let out = runner.run().await.unwrap();
        assert_eq!(out[0].1, 8);
    }

    /// Party 0 owns wire 0, party 1 owns wire 1.
    /// The two-pass send/receive handles cross-party input distribution.
    #[tokio::test]
    async fn two_party_input_sharing() {
        let mut stubs = stub_networks(2);
        let (net0, net1) = (stubs.remove(0), stubs.remove(0));
        let program = add_program();
        let p1 = program.clone();

        let inputs0 = vec![
            InputAssignment { wire: WireId(0), owner: 0, value: Some(10) },
            InputAssignment { wire: WireId(1), owner: 1, value: None },
        ];
        let inputs1 = vec![
            InputAssignment { wire: WireId(0), owner: 0, value: None },
            InputAssignment { wire: WireId(1), owner: 1, value: Some(20) },
        ];

        let t0 = tokio::spawn(async move {
            Runner::new(net0, ClearBackend::new(None), program, &inputs0)
                .unwrap()
                .run()
                .await
                .unwrap()
        });
        let t1 = tokio::spawn(async move {
            Runner::new(net1, ClearBackend::new(None), p1, &inputs1)
                .unwrap()
                .run()
                .await
                .unwrap()
        });

        assert_eq!(t0.await.unwrap()[0].1, 30);
        assert_eq!(t1.await.unwrap()[0].1, 30);
    }

    #[tokio::test]
    async fn three_party_input_sharing() {
        let mut stubs = stub_networks(3);
        let (net0, net1, net2) = {
            let mut it = stubs.drain(..);
            (it.next().unwrap(), it.next().unwrap(), it.next().unwrap())
        };
        let (p0, p1, p2) = {
            let p = add_program();
            (p.clone(), p.clone(), p)
        };

        // Party 0 owns wire 0 (7), party 1 owns wire 1 (13). Party 2 is compute-only.
        let mk = |w0_val: Option<u64>, w1_val: Option<u64>| {
            vec![
                InputAssignment { wire: WireId(0), owner: 0, value: w0_val },
                InputAssignment { wire: WireId(1), owner: 1, value: w1_val },
            ]
        };

        let t0 = tokio::spawn(async move {
            Runner::new(net0, ClearBackend::new(None), p0, &mk(Some(7), None))
                .unwrap().run().await.unwrap()
        });
        let t1 = tokio::spawn(async move {
            Runner::new(net1, ClearBackend::new(None), p1, &mk(None, Some(13)))
                .unwrap().run().await.unwrap()
        });
        let t2 = tokio::spawn(async move {
            Runner::new(net2, ClearBackend::new(None), p2, &mk(None, None))
                .unwrap().run().await.unwrap()
        });

        assert_eq!(t0.await.unwrap()[0].1, 20);
        assert_eq!(t1.await.unwrap()[0].1, 20);
        assert_eq!(t2.await.unwrap()[0].1, 20);
    }
}
