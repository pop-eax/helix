use ark_ff::{BigInteger, Field, FpConfig, One, PrimeField, Zero};
// Now we'll use the prime field underlying the BLS12-381 G1 curve.
use ark_test_curves::bls12_381::Fq as F;
use ark_std::UniformRand;
use ark_std::rand::rngs::OsRng;


pub fn rand_fr() -> F {
    let mut rng =  OsRng;
    F::rand(&mut rng)
}


pub fn share_secret(secret: F, threshold: u128) -> Vec<F> {
    let mut coeffs: Vec<F> = Vec::new();
    coeffs.push(secret);
    for _ in  0..threshold-1 {
        coeffs.push(rand_fr());
    }

    coeffs
}

 // Horner: (((a_{d} * x + a_{d-1}) * x + ...) * x + a0)
pub fn eval_poly<F: Field>(coeffs: &[F], x: F) -> F {
   
    let mut acc = F::zero();
    for &c in coeffs.iter().rev() {
        acc *= x;
        acc += c;
    }
    acc
}

pub fn generate_share(coeffs: &Vec<F>, i: u64) -> (F, F) {
    let x = F::from(i);
    let y = eval_poly(&coeffs, x);

    (x, y)
}

pub fn reconstruct_secret(shares: &Vec<(F, F)>) -> F {
    let mut secret = F::zero();

    for i in 0..shares.len() {
        let (x_i, y_i) = shares[i];

        let mut num = F::one();
        let mut den = F::one();

        for j in 0..shares.len() {
            if i != j {
                let (x_j, _) = shares[j];
                num *= -x_j;
                den *= x_i - x_j;
            }
        }

        let lambda_i = num * den.inverse().unwrap();
        secret += y_i * lambda_i;
    }

    secret
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_e2e() {
        let secret: F = rand_fr();
        let polynomial = share_secret(secret, 20);
        let mut shares:Vec<(F,F)> = Vec::new();
        for i in 0..3 {
            shares.push(generate_share(&polynomial, i))
        }
        let reconstruted_secret = reconstruct_secret(&shares);
        assert_eq!(reconstruted_secret, secret);
    }
}
