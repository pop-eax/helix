//! MNIST MPC inference benchmarks.
//!
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │ Group: bgw_networked_mnist                                                  │
//! │   full_784_3p  — 3-party BGW, 784-input MNIST (3 layers, 10 logit outputs) │
//! │                  Cycles through 10 real MNIST test images (one per digit).  │
//! │                  Accuracy printed before timing begins.                     │
//! │                                                                             │
//! │ Group: yao_mnist_tiny  (16-input MNIST with ReLU + argmax)                 │
//! │   single_process — Yao garbled circuit, single process                     │
//! │   networked_2p   — Full Yao 2P protocol + OT over in-memory channels       │
//! │                                                                             │
//! │ NOTE: Yao on the full 784-input MNIST would require ~17 M AND gates        │
//! │ (~35 min/image at current throughput) — impractical.  The tiny 4-class     │
//! │ network (16 inputs, quadrant detectors) is used instead for Yao.           │
//! └─────────────────────────────────────────────────────────────────────────────┘
//!
//! Run all:              cargo bench -p bench --bench mnist
//! BGW only:             cargo bench -p bench --bench mnist -- bgw_networked_mnist
//! Yao only:             cargo bench -p bench --bench mnist -- yao_mnist_tiny

mod data;

use std::{path::Path, time::{Duration, Instant}};

use bgw::{BgwNetBackend, PrimeField};
use criterion::{criterion_group, criterion_main, Criterion};
use garbledc::{backend::YaoBackend, ot::{OTReceiver, OTSender}};
use ir::lir::{Program, WireId};
use net::{stub_networks, NetworkLayer};
use runtime::{
    compile_to_vm_instructions,
    vm::{Backend, VMState},
    InputAssignment, Runner,
};
use serde::{Deserialize, Serialize};

// ─── Mersenne63 signed comparison ────────────────────────────────────────────

const M63: u64 = (1u64 << 63) - 1;
const HALF63: u64 = M63 / 2;

/// Compare two Mersenne63 field elements as signed integers.
/// Values > HALF63 are "negative" (they wrapped below 0).
fn signed_cmp(a: u64, b: u64) -> std::cmp::Ordering {
    match (a > HALF63, b > HALF63) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.cmp(&b),
    }
}

/// Return the index of the largest logit (signed field comparison).
fn signed_argmax(outputs: &[(WireId, u64)]) -> usize {
    outputs
        .iter()
        .enumerate()
        .max_by(|(_, (_, a)), (_, (_, b))| signed_cmp(*a, *b))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

// ─── helpers ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct InputsToml {
    inputs: InputsSection,
}
#[derive(Deserialize)]
struct InputsSection {
    values: Vec<u64>,
}

fn load_values(toml_path: &str) -> Vec<u64> {
    let text = std::fs::read_to_string(toml_path)
        .unwrap_or_else(|_| panic!("cannot read {toml_path}"));
    toml::from_str::<InputsToml>(&text)
        .unwrap_or_else(|e| panic!("TOML parse error in {toml_path}: {e}"))
        .inputs
        .values
}

fn load_circuit(ir_path: &str) -> Program {
    let bytes = std::fs::read(ir_path)
        .unwrap_or_else(|_| panic!("cannot read {ir_path}; compile with `helixc compile`"));
    Program::from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("circuit deserialize: {e}"))
}

fn n_wires(prog: &Program) -> usize {
    prog.circuit
        .gates
        .iter()
        .map(|g| g.output.0 as usize)
        .chain(prog.circuit.inputs.iter().map(|i| i.wire.0 as usize))
        .max()
        .unwrap_or(0)
        + 1
}

fn field_mod(prog: &Program) -> u64 {
    prog.metadata.field_modulus.unwrap_or(M63)
}

/// Round-robin ownership across `n_parties`.
fn assignments(prog: &Program, vals: &[u64], n: usize, my_id: usize) -> Vec<InputAssignment> {
    prog.circuit
        .inputs
        .iter()
        .zip(vals.iter())
        .enumerate()
        .map(|(idx, (inp, &v))| {
            let owner = idx % n;
            InputAssignment {
                wire: inp.wire,
                owner,
                value: if owner == my_id { Some(v) } else { None },
            }
        })
        .collect()
}

/// Build a complete input vector for the MNIST linear circuit by substituting
/// `pixels` (784 encoded values) into the last 784 slots of `base_vals`.
fn build_mnist_vals(base_vals: &[u64], pixels: &[u64]) -> Vec<u64> {
    let n_weights = base_vals.len() - 784;
    let mut vals = base_vals.to_vec();
    vals[n_weights..].copy_from_slice(pixels);
    vals
}

// ─── BGW networked ────────────────────────────────────────────────────────────

/// Run 3-party BGW on `prog` with `vals` as inputs.
/// Returns the output wire values from party 0 (all parties agree on outputs).
async fn bgw_run(prog: &Program, vals: &[u64], n: usize, t: usize) -> Vec<(WireId, u64)> {
    let field = PrimeField::new(field_mod(prog));
    let n_muls = bgw::count_multiplications(prog);
    let blobs = bgw::dealer_generate_triple_blobs(n_muls, n, t, &field);
    let triple_shares: Vec<_> =
        blobs.iter().map(|b| bgw::parse_triple_blob(b).unwrap()).collect();

    let mut stubs = stub_networks(n);
    let handles: Vec<_> = (0..n)
        .map(|id| {
            let stub = stubs.remove(0);
            let triples = triple_shares[id].clone();
            let backend = BgwNetBackend::new(id, n, t, field, triples).unwrap();
            let inputs = assignments(prog, vals, n, id);
            let prog = prog.clone();
            tokio::spawn(async move {
                Runner::new(stub, backend, prog, &inputs).unwrap().run().await.unwrap()
            })
        })
        .collect();

    let mut party0_outputs = Vec::new();
    for (id, h) in handles.into_iter().enumerate() {
        let out = h.await.unwrap();
        if id == 0 {
            party0_outputs = out;
        }
    }
    party0_outputs
}

// ─── Yao single-process ───────────────────────────────────────────────────────

fn yao_single(prog: &Program, vals: &[u64], bits: usize) -> Vec<(WireId, u64)> {
    let mut backend = YaoBackend::new(bits);
    let instructions = compile_to_vm_instructions(&prog.circuit);
    let mut state = VMState::new(n_wires(prog), field_mod(prog));
    for instr in &instructions {
        backend.execute_instruction(instr, &mut state).unwrap();
    }
    for (inp, &v) in prog.circuit.inputs.iter().zip(vals.iter()) {
        backend
            .set_input(inp.wire, v, runtime::Visibility::Secret, &mut state)
            .unwrap();
    }
    let (gc, active_labels, decode) = backend.finalize_garbler();
    let final_labels = gc.evaluate(active_labels);
    prog.circuit
        .outputs
        .iter()
        .map(|&w| {
            let base = backend.wire_base_slot(w);
            let mut val = 0u64;
            for b in 0..bits {
                if let Some(lbl) = final_labels.get(base + b).and_then(|x| *x) {
                    val |= (((lbl & 1) ^ decode[base + b] as u128) as u64) << b;
                }
            }
            (w, val)
        })
        .collect()
}

// ─── Yao 2P networked ────────────────────────────────────────────────────────

async fn net_send<T: Serialize>(n: &mut impl NetworkLayer, to: usize, v: &T) {
    n.send_to(to, bincode::serialize(v).unwrap()).await.unwrap();
}
async fn net_recv<T: for<'de> Deserialize<'de>>(n: &mut impl NetworkLayer, from: usize) -> T {
    bincode::deserialize(&n.recv_from(from).await.unwrap()).unwrap()
}

#[derive(Serialize, Deserialize)]
struct GarblerBundle {
    gc:                garbledc::circuit::Circuit,
    active:            Vec<Option<u128>>,
    decode:            Vec<u8>,
    ot:                Vec<(u128, u128)>,
    eval_base_slots:   Vec<usize>,
    output_base_slots: Vec<usize>,
}

async fn garbler(
    prog: Program,
    vals: Vec<(WireId, u64)>,
    eval_wires: Vec<WireId>,
    bits: usize,
    mut net: impl NetworkLayer,
) {
    let mut backend = YaoBackend::new(bits);
    let instructions = compile_to_vm_instructions(&prog.circuit);
    let mut state = VMState::new(n_wires(&prog), field_mod(&prog));
    for instr in &instructions {
        backend.execute_instruction(instr, &mut state).unwrap();
    }
    for &(w, v) in &vals {
        backend
            .set_input(w, v, runtime::Visibility::Secret, &mut state)
            .unwrap();
    }
    let mut ot_msgs: Vec<(u128, u128)> = Vec::new();
    let mut eval_base_slots: Vec<usize> = Vec::new();
    for &w in &eval_wires {
        backend.register_evaluator_wire(w);
        eval_base_slots.push(backend.wire_base_slot(w));
        for b in 0..bits {
            let [l0, l1] = backend.wire_label_pair(w, b).unwrap();
            ot_msgs.push((l0, l1));
        }
    }
    let output_base_slots: Vec<usize> = prog.circuit.outputs.iter()
        .map(|&w| backend.wire_base_slot(w))
        .collect();
    let (ot_sender, a) = OTSender::setup(ot_msgs.len());
    net_send(&mut net, 1, &a).await;
    let b_bytes: Vec<[u8; 32]> = net_recv(&mut net, 1).await;
    let ot = ot_sender.respond(&b_bytes, &ot_msgs);
    let (gc, active, decode) = backend.finalize_garbler();
    net_send(&mut net, 1, &GarblerBundle { gc, active, decode, ot, eval_base_slots, output_base_slots }).await;
    let _: Vec<u64> = net_recv(&mut net, 1).await;
}

async fn evaluator(
    _prog: Program,
    vals: Vec<(WireId, u64)>,
    bits: usize,
    mut net: impl NetworkLayer,
) {
    let choices: Vec<bool> = vals
        .iter()
        .flat_map(|&(_, v)| (0..bits).map(move |i| (v >> i) & 1 == 1))
        .collect();
    let a: Vec<[u8; 32]> = net_recv(&mut net, 0).await;
    let (ot_recv, b) = OTReceiver::choose(&a, &choices);
    net_send(&mut net, 0, &b).await;
    let bundle: GarblerBundle = net_recv(&mut net, 0).await;
    let my_labels = ot_recv.finish(&bundle.ot);
    let mut active = bundle.active;
    for (i, &base) in bundle.eval_base_slots.iter().enumerate() {
        for b in 0..bits {
            active[base + b] = Some(my_labels[i * bits + b]);
        }
    }
    let final_labels = bundle.gc.evaluate(active);
    let outputs: Vec<u64> = bundle.output_base_slots.iter()
        .map(|&base| {
            let mut val = 0u64;
            for b in 0..bits {
                if let Some(lbl) = final_labels.get(base + b).and_then(|x| *x) {
                    val |= (((lbl & 1) ^ bundle.decode[base + b] as u128) as u64) << b;
                }
            }
            val
        })
        .collect();
    net_send(&mut net, 0, &outputs).await;
}

async fn yao_2p_run(prog: &Program, vals: &[u64], bits: usize) {
    let all: Vec<(WireId, u64)> = prog
        .circuit
        .inputs
        .iter()
        .zip(vals.iter())
        .map(|(i, &v)| (i.wire, v))
        .collect();
    let garbler_vals: Vec<_> = all
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(_, &v)| v)
        .collect();
    let eval_vals: Vec<_> = all
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, &v)| v)
        .collect();
    let eval_wires: Vec<_> = all
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, &(w, _))| w)
        .collect();

    let mut stubs = stub_networks(2);
    let s0 = stubs.remove(0);
    let s1 = stubs.remove(0);
    let p0 = prog.clone();
    let p1 = prog.clone();
    tokio::try_join!(
        tokio::spawn(async move { garbler(p0, garbler_vals, eval_wires, bits, s0).await }),
        tokio::spawn(async move { evaluator(p1, eval_vals, bits, s1).await }),
    )
    .unwrap();
}

// ─── Yao tiny: 4 canonical test patterns ─────────────────────────────────────

/// Canonical test patterns for the 4-class tiny MNIST.
/// Each pattern lights up one 2×2 quadrant of the 4×4 image.
/// Pixel layout: pixels[row*4 + col].
const YAO_PATTERNS: [[u64; 16]; 4] = [
    // Class 0: top-left  (rows 0-1, cols 0-1)
    [255, 255, 0, 0,  255, 255, 0, 0,  0, 0, 0, 0,  0, 0, 0, 0],
    // Class 1: top-right (rows 0-1, cols 2-3)
    [0, 0, 255, 255,  0, 0, 255, 255,  0, 0, 0, 0,  0, 0, 0, 0],
    // Class 2: bottom-left  (rows 2-3, cols 0-1)
    [0, 0, 0, 0,  0, 0, 0, 0,  255, 255, 0, 0,  255, 255, 0, 0],
    // Class 3: bottom-right (rows 2-3, cols 2-3)
    [0, 0, 0, 0,  0, 0, 0, 0,  0, 0, 255, 255,  0, 0, 255, 255],
];

// ─── Criterion groups ─────────────────────────────────────────────────────────

fn bench_bgw_mnist(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

    let prog = load_circuit(&root.join("tests/samples/mnist_linear.ir").to_string_lossy());
    let base_vals =
        load_values(&root.join("tests/samples/mnist_linear.toml").to_string_lossy());
    let mnist = data::load(&root.join("bench/data"));

    // ── Accuracy pre-check ────────────────────────────────────────────────────
    // Run all 10 real MNIST test images once (outside Criterion timing) to
    // verify correctness before benchmarking.
    eprint!("\nBGW MNIST accuracy check ({} images)... ", mnist.images.len());
    let mut correct = 0usize;
    for (img_idx, (pixels, &label)) in mnist.images.iter().zip(mnist.labels.iter()).enumerate() {
        let vals = build_mnist_vals(&base_vals, pixels);
        let outputs = rt.block_on(bgw_run(&prog, &vals, 3, 1));
        let predicted = signed_argmax(&outputs);
        if predicted == label {
            correct += 1;
        }
        eprint!("{}", if predicted == label { "✓" } else { "✗" });
        let _ = img_idx;
    }
    eprintln!(" → {correct}/{} correct", mnist.images.len());

    // ── Criterion timing ─────────────────────────────────────────────────────
    // Each iteration picks the next image round-robin.
    // Only the BGW execution time is counted; input construction is in setup.
    let mut group = c.benchmark_group("bgw_networked_mnist");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(300));

    let n_images = mnist.images.len();
    let mut img_idx = 0usize;
    group.bench_function("full_784_3p_t1", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let pixels = &mnist.images[img_idx % n_images];
                img_idx += 1;
                let vals = build_mnist_vals(&base_vals, pixels);
                let start = Instant::now();
                rt.block_on(bgw_run(&prog, &vals, 3, 1));
                total += start.elapsed();
            }
            total
        });
    });

    group.finish();
}

fn bench_yao(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

    let prog = load_circuit(&root.join("tests/samples/mnist.ir").to_string_lossy());

    // The tiny MNIST logits peak at 64 * (4 * 64 * 255) = 4,177,920, which
    // requires 23 bits.  Use 24 bits so all arithmetic is exact.
    // (8-bit Yao would produce 0 for every logit due to mod-256 overflow,
    //  causing argmax to always return 0 regardless of the input pattern.)
    const YAO_BITS: usize = 24;

    // ── Accuracy pre-check (4 canonical patterns, 4 classes) ─────────────────
    eprint!("\nYao tiny MNIST accuracy check (4 patterns, {YAO_BITS}-bit)... ");
    let mut correct = 0usize;
    for (cls, pattern) in YAO_PATTERNS.iter().enumerate() {
        let outputs = yao_single(&prog, pattern, YAO_BITS);
        let predicted = outputs.first().map(|&(_, v)| v as usize).unwrap_or(99);
        if predicted == cls {
            correct += 1;
        }
        eprint!("{}", if predicted == cls { "✓" } else { "✗" });
    }
    eprintln!(" → {correct}/4 correct");

    // Use bottom-right pattern (class 3) for timed benchmark.
    let vals: Vec<u64> = YAO_PATTERNS[3].to_vec();

    let mut group = c.benchmark_group("yao_mnist_tiny");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(90));

    group.bench_function("single_process", |b| {
        b.iter(|| yao_single(&prog, &vals, YAO_BITS));
    });

    group.bench_function("networked_2p", |b| {
        b.to_async(&rt).iter(|| yao_2p_run(&prog, &vals, YAO_BITS));
    });

    group.finish();
}

criterion_group!(benches, bench_bgw_mnist, bench_yao);
criterion_main!(benches);
