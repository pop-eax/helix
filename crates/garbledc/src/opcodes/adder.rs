use crate::circuit::Circuit;
use crate::gate::{xor_logic, and_logic, or_logic};

/// Build an 8-bit ripple-carry adder into `circuit`.
///
/// Slot layout (relative to `start_slot`):
///   a[i]  = start_slot + i        (i = 0..8) — circuit inputs
///   b[i]  = start_slot + 8 + i   (i = 0..8) — circuit inputs
///   Intermediate sum/carry/carry-propagate slots begin at start_slot + 16.
///
/// Outputs: sum[0..8] and the final carry-out, registered as circuit outputs.
pub fn build_8bit_adder(circuit: &mut Circuit, start_slot: usize) {
    // Register input slots.
    for i in 0..8 {
        circuit.add_input_slot(start_slot + i);      // a[i]
        circuit.add_input_slot(start_slot + 8 + i);  // b[i]
    }

    // Allocate intermediate slots starting after the 16 input slots.
    let mut next = start_slot + 16;
    let mut alloc = || -> usize {
        let s = next;
        next += 1;
        s
    };

    let a0 = start_slot;
    let b0 = start_slot + 8;
    let sum0 = alloc();
    let c0   = alloc();

    circuit.add_gate(xor_logic(), &[a0, b0], sum0);
    circuit.add_gate(and_logic(), &[a0, b0], c0);
    circuit.add_output_slot(sum0);

    let mut carry = c0;
    for i in 1..8 {
        let a = start_slot + i;
        let b = start_slot + 8 + i;
        let sum         = alloc();
        let cout        = alloc();
        let a_xor_b     = alloc();
        let a_and_b     = alloc();
        let cin_axorb   = alloc();

        circuit.add_gate(xor_logic(), &[a, b],           a_xor_b);
        circuit.add_gate(xor_logic(), &[a_xor_b, carry], sum);
        circuit.add_gate(and_logic(), &[a, b],            a_and_b);
        circuit.add_gate(and_logic(), &[carry, a_xor_b],  cin_axorb);
        circuit.add_gate(or_logic(),  &[a_and_b, cin_axorb], cout);
        circuit.add_output_slot(sum);
        carry = cout;
    }

    circuit.add_output_slot(carry); // carry-out
}
