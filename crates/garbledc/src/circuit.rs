use super::gate::Gate;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use rayon::prelude::*;


#[derive(Clone, Serialize, Deserialize)]
pub struct Circuit {
    pub labels: HashMap<String, [u128; 2]>,
    pub gates: Vec<Gate>,

    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

impl Circuit {
    pub fn new() -> Self {
        Self {
            labels: HashMap::new(),
            gates: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    pub fn get_or_create_labels(&mut self, wire_name: &str) -> [u128; 2] {
        if let Some(&labels) = self.labels.get(wire_name) {
            labels
        } else {
            // Enforce color-bit convention: lsb(label₀) = 0, lsb(label₁) = 1.
            // This means the active label's LSB is the wire's truth value, so
            // output decoding only requires one bit per wire (lsb(label₀)).
            let label0 = super::random_label() & !1u128;
            let label1 = super::random_label() | 1u128;
            let new_labels = [label0, label1];
            self.labels.insert(wire_name.to_string(), new_labels);
            new_labels
        }
    }

    pub fn add_input(&mut self, name: &str){
        self.get_or_create_labels(name);
        if !self.inputs.contains(&name.to_string()) {
            self.inputs.push(name.to_string());
        }
    }

    pub fn add_output(&mut self, name: &str) {
        self.get_or_create_labels(name);
        if !self.outputs.contains(&name.to_string()) {
            self.outputs.push(name.to_string());
        }
    }

    pub fn add_gate(&mut self, logic_table: Vec<u8>, inputs: &[&str], output: &str) {
        for &input in inputs {
            self.get_or_create_labels(input);
        }
        self.get_or_create_labels(output);

        let input_names: Vec<String> = inputs.iter().map(|s| s.to_string()).collect();
        let mut gate = Gate::new(logic_table, input_names, output.to_string());

        gate.label_table(&self.labels);
        self.gates.push(gate);
    }

    pub fn garble(&mut self) -> Vec<Vec<(Vec<u8>, Vec<u8>)>> {
        self.gates
            .iter_mut()
            .map(|gate| gate.garble_table())
            .collect()
    }

    pub fn evaluate(&self, mut active_labels: HashMap<String, u128>) -> HashMap<String, u128> {
        for gate in &self.gates {
            let input_labels: Vec<u128> = gate
                .input_labels()
                .iter()
                .map(|name| active_labels[name])
                .collect();

            if let Some(output_label) = gate.clone().evaluate(input_labels) {
                active_labels.insert(gate.output_label().to_string(), output_label);
            }
        }

        active_labels
    }

    /// Compute the topological level of each gate in the boolean circuit.
    /// Returns a Vec parallel to `self.gates` (gate i → its level).
    /// Input wires (those in `self.inputs`) are at level 0.
    pub fn compute_gate_levels(&self) -> Vec<u32> {
        let mut wire_level: HashMap<&str, u32> = self
            .inputs
            .iter()
            .map(|name| (name.as_str(), 0u32))
            .collect();

        let mut levels = Vec::with_capacity(self.gates.len());
        for gate in &self.gates {
            let lv = gate
                .input_labels()
                .iter()
                .map(|name| wire_level.get(name.as_str()).copied().unwrap_or(0))
                .max()
                .unwrap_or(0)
                + 1;
            wire_level.insert(gate.output_label(), lv);
            levels.push(lv);
        }
        levels
    }

    /// Level-parallel evaluation using rayon.
    ///
    /// Gates at the same topological level are independent and evaluated
    /// concurrently.  Suitable for circuits up to a few million gates; for
    /// very large circuits the level-computation overhead may dominate.
    pub fn evaluate_parallel(&self, mut active_labels: HashMap<String, u128>) -> HashMap<String, u128> {
        if self.gates.is_empty() {
            return active_labels;
        }

        let gate_levels = self.compute_gate_levels();
        let max_level = *gate_levels.iter().max().unwrap() as usize;

        // Group gate indices by level.
        let mut by_level: Vec<Vec<usize>> = vec![Vec::new(); max_level + 1];
        for (idx, &lv) in gate_levels.iter().enumerate() {
            by_level[lv as usize].push(idx);
        }

        for level_indices in &by_level {
            // Parallel: each gate reads from active_labels (all earlier levels).
            let new_labels: Vec<(String, u128)> = level_indices
                .par_iter()
                .filter_map(|&idx| {
                    let gate = &self.gates[idx];
                    let inputs: Vec<u128> = gate
                        .input_labels()
                        .iter()
                        .map(|n| active_labels[n.as_str()])
                        .collect();
                    gate.clone().evaluate(inputs).map(|lbl| (gate.output_label().to_string(), lbl))
                })
                .collect();
            // Sequential write: each wire is written by exactly one gate.
            active_labels.extend(new_labels);
        }

        active_labels
    }

    pub fn get_label(&self, wire_name: &str, bit: u8) -> Option<u128> {
        self.labels.get(wire_name).map(|labels| labels[bit as usize])
    }
    
    pub fn print_structure(&self) {
        println!("=== Circuit Structure ===");
        println!("Inputs: {:?}", self.inputs);
        println!("Outputs: {:?}", self.outputs);
        println!("\nGates:");
        for (i, gate) in self.gates.iter().enumerate() {
            println!("  Gate {}: {:?} -> {}", 
                i, gate.input_labels(), gate.output_label());
        }
        println!("\nLabels: {} wires", self.labels.len());
    }
}