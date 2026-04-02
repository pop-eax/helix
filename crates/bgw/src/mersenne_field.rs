use ark_ff::{Fp64, MontBackend, MontConfig};

/// 64-bit prime field with modulus 2^63 − 1 (Mersenne prime).
///
/// This matches the runtime's field modulus (`Field<64>`) so that BGW
/// arithmetic gives the same results as the clear backend.
#[derive(MontConfig)]
#[modulus = "9223372036854775807"]
#[generator = "3"]
pub struct Mersenne63Config;

pub type Mersenne63 = Fp64<MontBackend<Mersenne63Config, 1>>;
