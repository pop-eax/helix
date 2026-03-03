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

pub fn generate_share(coeffs: &Vec<F>, i: u128) -> (F, F) {
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

pub fn add_secrets(a: &Vec<(F, F)>, b: &Vec<(F, F)>) -> Vec<(F, F)> {
    assert_eq!(a.len(), b.len());
    a
    .iter()
    .zip(b.iter())
    .map(|((x1, y1), (_, y2))| (*x1, *y1+*y2) )
    .collect()
}

pub fn subtract_secrets(a: &Vec<(F, F)>, b: &Vec<(F, F)>) -> Vec<(F, F)> {
    assert_eq!(a.len(), b.len());
    a
    .iter()
    .zip(b.iter())
    .map(|((x1, y1), (_, y2))| (*x1, *y1-*y2) )
    .collect()
}

fn scale_vector(a: &Vec<(F, F)>, k: F) -> Vec<(F, F)>{
    a
    .iter()
    .map(|(x1, y1)| (*x1, *y1 * k) )
    .collect()
}

//beaver triples
pub fn mutiply_secrets(x: &Vec<(F, F)>, y: &Vec<(F, F)>) -> Vec<(F, F)> {
    let a = rand_fr();
    let b = rand_fr();
    let c = a * b;
    let t = x.len() as u128;
    let a_secret = share_secret(a, t);
    let b_secret = share_secret(b, t);
    let c_secret = share_secret(c, t);
    let mut a_shares:Vec<(F,F)> = Vec::new();
    let mut b_shares:Vec<(F,F)> = Vec::new();
    let mut c_shares:Vec<(F,F)> = Vec::new();
    for i in 1..t+1 {
        a_shares.push(generate_share(&a_secret, i));
        b_shares.push(generate_share(&b_secret, i));
        c_shares.push(generate_share(&c_secret, i));
    }
    let delta = reconstruct_secret(&subtract_secrets(&x, &a_shares));
    let eta = reconstruct_secret(&subtract_secrets(&y, &b_shares));

    // [z]= [c]+ δ⋅[b]+ϵ⋅[a]+δ⋅ϵ
    let z1 = scale_vector(&b_shares, delta);
    let z2 = scale_vector(&a_shares, eta);

    add_secrets(&c_shares,(&add_secrets(&z1, &z2)))
    .iter()
    .map(|(x1, y1)| (*x1, *y1+(delta*eta)))
    .collect()
}




#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharing() {
        let secret: F = rand_fr();
        let threshold: u128 = 2000;
        let polynomial = share_secret(secret, threshold.into());
        let mut shares:Vec<(F,F)> = Vec::new();
        for i in 1..threshold+1 {
            shares.push(generate_share(&polynomial, i));
        }
        let reconstruted_secret = reconstruct_secret(&shares);
        println!("{} {}", secret, reconstruted_secret);
        assert_eq!(reconstruted_secret, secret);
    }
    
    #[test]
    fn test_addition() {
        let a: F = rand_fr();
        let b: F = rand_fr();
        let threshold: u128 = 200;
        let p1 = share_secret(a, threshold.into());
        let p2 = share_secret(b, threshold.into());
        let mut shares1:Vec<(F,F)> = Vec::new();
        let mut shares2:Vec<(F,F)> = Vec::new();
        for i in 1..threshold+1 {
            shares1.push(generate_share(&p1, i));
            shares2.push(generate_share(&p2, i));
        }
        let secret_shares = add_secrets(&shares1, &shares2);
        let secret_sum = reconstruct_secret(&secret_shares);
        let clear_sum = a + b;
        println!("{}+{} = {} {}", a, b, secret_sum, clear_sum);
        assert_eq!(clear_sum, secret_sum);
    }


    #[test]
    fn test_subtraction() {
        let a: F = rand_fr();
        let b: F = rand_fr();
        let threshold: u128 = 200;
        let p1 = share_secret(a, threshold.into());
        let p2 = share_secret(b, threshold.into());
        let mut shares1:Vec<(F,F)> = Vec::new();
        let mut shares2:Vec<(F,F)> = Vec::new();
        for i in 1..threshold+1 {
            shares1.push(generate_share(&p1, i));
            shares2.push(generate_share(&p2, i));
        }
        let secret_shares = subtract_secrets(&shares1, &shares2);
        let secret_sum = reconstruct_secret(&secret_shares);
        let clear_sum = a - b;
        println!("{} - {} = {} {}", a, b, secret_sum, clear_sum);
        assert_eq!(clear_sum, secret_sum);
    }

    #[test]
    fn test_multiplication() {
        let a: F = rand_fr();
        let b: F = rand_fr();
        let threshold: u128 = 200;
        let p1 = share_secret(a, threshold.into());
        let p2 = share_secret(b, threshold.into());
        let mut shares1:Vec<(F,F)> = Vec::new();
        let mut shares2:Vec<(F,F)> = Vec::new();
        for i in 1..threshold+1 {
            shares1.push(generate_share(&p1, i));
            shares2.push(generate_share(&p2, i));
        }
        let secret_shares = mutiply_secrets(&shares1, &shares2);
        let secret_sum = reconstruct_secret(&secret_shares);
        let clear_sum = a * b;
        println!("{} * {} = {} {}", a, b, secret_sum, clear_sum);
        assert_eq!(clear_sum, secret_sum);
    }

}
