pub mod gate;
pub mod circuit;
pub mod opcodes;
pub mod backend;

use rand;
use rand::{Rng};

pub fn random_label() -> u128 {
    rand::rng().random()
}