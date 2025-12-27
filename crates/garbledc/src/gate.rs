use aes_gcm::{
    Aes256Gcm, Key, aead::{Aead, AeadCore, KeyInit, OsRng},
};
use itertools::Itertools;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use rand::{seq::SliceRandom};
use rand::rng;
use serde::{Serialize, Deserialize};
pub use super::random_label;

fn inputs_to_index(inputs: &[u8]) -> usize {
    inputs
        .iter()
        .rev()
        .enumerate()
        .fold(0, |acc, (i, &bit)| acc + (bit as usize) * (1 << i))
}

pub fn combine_keys(keys: Vec<u128>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for k in keys {
        hasher.update(k.to_le_bytes());
    }

    hasher.finalize().into()
}

pub fn encrypt(k: &[u8; 32], pt: [u8; 16]) -> (Vec<u8>, Vec<u8>) {
    // let key = Key::<Aes128Gcm>::from_slice(k);
    let cipher = Aes256Gcm::new_from_slice(k).unwrap();
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, pt.as_ref()).unwrap();
    (nonce.to_vec(), ciphertext)
}

pub fn decrypt(
    k: &[u8; 32],
    n: Vec<u8>,
    ct: &[u8],
) -> Option<u128> {
    let nonce = <&aes_gcm::Nonce::<aes_gcm::aead::consts::U12>>::from(n.as_slice());

    let key = Key::<Aes256Gcm>::from_slice(k);
    let cipher = Aes256Gcm::new(&key);
    let plaintext = cipher.decrypt(nonce, ct).ok()?;
    Some(u128::from_be_bytes(plaintext.try_into().ok()?))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Gate {
    // compute_fn: Box<dyn Fn(&[bool]) -> bool + Send + Sync>,
    // is_free: bool,
    logic_table: Vec<u8>,
    // pub labels: HashMap<String, [u128; 2]>,
    truth_table: Vec<(Vec<u128>, u128)>,
    // garbled_table: Vec<(Vec<u128>, (Vec<u8>, Vec<u8>))>,
    garbled_table: Vec<(Vec<u8>, Vec<u8>)>,
    input_labels: Vec<String>,
    output_label: String
}

impl Gate {
    pub fn new(
        // is_free: bool,
        logic_table: Vec<u8>,
        input_names: Vec<String>,
        output_name: String
    ) -> Self {
        Self {
            // is_free,
            logic_table,
            // labels: HashMap::new(),
            truth_table: Vec::new(),
            garbled_table: Vec::new(),
            input_labels: input_names,
            output_label: output_name
        }
    }

    pub fn label_table(
        &mut self,
        circuit_labels: &HashMap<String, [u128; 2]>
    ) -> (Vec<u128>, Vec<Vec<u128>>) {
        // let label_values: Vec<[u128; 2]> = (0..input_labels.len())
        //     .map(|_| [random_label(), random_label()])
        //     .collect();

        // let output_labels: [u128; 2] = [random_label(), random_label()];
        // self.labels.insert(out_label, output_labels);
        // for i in 0..input_labels.len() {
        //     let l = input_labels.get(i).unwrap().to_string();
        //     let v = *label_values.get(i).unwrap();
        //     self.labels.insert(l, v);
        // }
        let output_labels = circuit_labels[&self.output_label];

        let truth_table: Vec<Vec<u8>> = vec![vec![0, 1]; self.input_labels.len()]
            .into_iter()
            .multi_cartesian_product()
            .collect();

        let labeled_tabled: Vec<Vec<u128>> = truth_table
            .iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .map(|(idx, val)| circuit_labels.get(&self.input_labels[idx]).unwrap()[*val as usize])
                    .collect()
            })
            .collect();

        let outputs: Vec<u128> = truth_table
            .iter()
            .map(|inputs| output_labels[self.logic_table[inputs_to_index(inputs)] as usize])
            .collect();

        self.truth_table = (0..labeled_tabled.len())
            .map(|i| (labeled_tabled[i].clone(), outputs[i]))
            .collect();

        (outputs, labeled_tabled)
    }

    pub fn garble_table(&mut self) -> Vec<(Vec<u8>, Vec<u8>)>{
        //  aes_gcm::Nonce<aes_gcm::aead::consts::U12>
        // member to shuffle
        self.garbled_table = self
            .truth_table
            .iter()
            .map(|(row, output)| {
                let key = combine_keys(row.clone());
                let output_id: [u8; 16] = output.to_be_bytes();
                let encrypted_id = encrypt(&key, output_id);
                encrypted_id
            })
            .collect();

        self.garbled_table.shuffle(&mut rng());
        self.garbled_table.clone()
    }

    pub fn evaluate(self, inputs: Vec<u128>) -> Option<u128> {
        let key = combine_keys(inputs);
        for (nonce, ct) in self.garbled_table {
            let t = decrypt(&key, nonce, ct.as_slice());
            match t {
                Some(x) => {return Some(x)},
                None => {},
            }
        }
        None
    }

    pub fn input_labels(&self) -> &[String] {
        &self.input_labels
    }
    
    pub fn output_label(&self) -> &str {
        &self.output_label
    }
}

pub fn and_logic() -> Vec<u8> {
    vec![0, 0, 0, 1]
}

pub fn or_logic() -> Vec<u8> {
    vec![0, 1, 1, 1]
}

pub fn xor_logic() -> Vec<u8> {
    vec![0, 1, 1, 0]
}

pub fn not_logic() -> Vec<u8> {
    vec![1, 0]
}

pub fn nand_logic() -> Vec<u8> {
    vec![1, 1, 1, 0]
}