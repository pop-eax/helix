use crate::circuit::{Circuit};
use crate::gate::{xor_logic, and_logic, or_logic};

pub fn build_8bit_adder(circuit: &mut Circuit) {
    
    for i in 0..8 {
        circuit.add_input(&format!("a{}", i));
        circuit.add_input(&format!("b{}", i));
    }
    
    circuit.add_gate(xor_logic(), &["a0", "b0"], "sum0");
    circuit.add_gate(and_logic(), &["a0", "b0"], "c0");
    circuit.add_output("sum0");
    
    for i in 1..8 {
        let a = format!("a{}", i);
        let b = format!("b{}", i);
        let cin = format!("c{}", i - 1);
        let sum = format!("sum{}", i);
        let cout = format!("c{}", i);
        
        let a_xor_b = format!("a_xor_b_{}", i);
        let a_and_b = format!("a_and_b_{}", i);
        let cin_and_axorb = format!("cin_and_axorb_{}", i);
        
        circuit.add_gate(xor_logic(), &[&a, &b], &a_xor_b);
        circuit.add_gate(xor_logic(), &[&a_xor_b, &cin], &sum);
        
        circuit.add_gate(and_logic(), &[&a, &b], &a_and_b);
        circuit.add_gate(and_logic(), &[&cin, &a_xor_b], &cin_and_axorb);
        circuit.add_gate(or_logic(), &[&a_and_b, &cin_and_axorb], &cout);
        
        circuit.add_output(&sum);
    }
    
    circuit.add_output("c7");
}