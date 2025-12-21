pub mod gate;
pub use gate::Gate;
use rand;
use rand::{seq::SliceRandom, Rng};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

pub fn random_label() -> u128 {
    rand::rng().random()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Circuit {
    pub labels: HashMap<String, [u128; 2]>,
    gates: Vec<Gate>,

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
            let new_labels = [random_label(), random_label()];
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
