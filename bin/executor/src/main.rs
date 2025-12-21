use std::collections::HashMap;
// use garbledc::gate::{Gate, and_gate, xor_gate, not_gate};
use itertools::Itertools;
use garbledc::{Circuit};
use garbledc::gate::{and_logic, xor_logic};

fn main() {
    // let mut not_gate = not_gate();
    // // Gate::new(vec![0,0,0,1]);
    // let (nout_labels, not_labels) = not_gate
    // .label_table(&[String::from("A")], "Out".to_string());

    // for (row, out) in not_labels.iter().zip(nout_labels) {
    //     println!("{} | {}", row.into_iter().join(" | "), out);
    // }

    // println!("--------------------------GARBLED NOT---------------------------");

    // let ngarbled_res = not_gate.garble_table();

    // for (row, (n, out)) in ngarbled_res {
    //     println!("{} | {}", row.into_iter().join(" | "), hex::encode(out));
    // }
    // let ls = not_gate.labels.clone();
    // let res =not_gate.evaluate(vec![ls["A"][1]]);
    // println!("=============================================RESULT======================");
    // println!("{} => {}", res, ls["Out"].iter().find_position(|&x| *x == res).unwrap().0);

    // println!("=====================================================");

    // let mut and_gate = and_gate();
    // // Gate::new(vec![0,0,0,1]);
    // let (out_labels, and_labels) = and_gate
    // .label_table(&[String::from("A"), String::from("B")], "Out".to_string());

    // for (row, out) in and_labels.iter().zip(out_labels) {
    //     println!("{} | {}", row.into_iter().join(" | "), out);
    // }

    // println!("--------------------------GARBLED---------------------------");

    // let garbled_res = and_gate.garble_table();

    // for (row, (n, out)) in garbled_res {
    //     println!("{} | {}", row.into_iter().join(" | "), hex::encode(out));
    // }
    // let ls = and_gate.labels.clone();
    // let res = and_gate.evaluate(vec![ls["A"][1], ls["B"][0]]);
    // println!("=============================================RESULT======================");
    // println!("{} => {}", res, ls["Out"].iter().find_position(|&x| *x == res).unwrap().0);

    let mut circuit = Circuit::new();

    circuit.add_input("a");
    circuit.add_input("b");
    circuit.add_input("c");

    circuit.add_gate(and_logic(), &["a", "b"], "temp1");
    circuit.add_gate(xor_logic(), &["temp1", "c"], "output");

    circuit.add_output("output");

    circuit.print_structure();

    let garbled_tables = circuit.garble();
    println!("Garbled {} gates", garbled_tables.len());

    let mut active_labels = HashMap::new();
    active_labels.insert("a".to_string(), circuit.get_label("a", 1).unwrap());
    active_labels.insert("b".to_string(), circuit.get_label("b", 0).unwrap());
    active_labels.insert("c".to_string(), circuit.get_label("c", 1).unwrap());

    let results = circuit.evaluate(active_labels);
    let output_label = results["output"];

    let output_bit = if output_label == circuit.labels["output"][0] {
        0
    } else {
        1
    };
    println!("Output: {}", output_bit); // (1 AND 0) XOR 1 = 0 XOR 1 = 1
}
