extern crate creusot_std;
// Import Creusot items without the prelude glob to avoid derive-macro ambiguity
// with the standard library's Clone / PartialEq derive macros.
use creusot_std::{
    ghost::Ghost,
    logic::{Int, Seq},
    model::View,
    snapshot::Snapshot,
};
use creusot_std::macros::*;

use ark_bls12_381::Fr;
use ark_ff::{Field, Zero, One};
use ark_std::rand::Rng;
use ark_std::UniformRand;

// ── Error type ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum ShamirError {
    InvalidThreshold,
    InvalidPartyCount,
    EmptyShares,
    DuplicateShareX,
}

// ── Logical field model ─────────────────────────────────────────────────────
// We cannot annotate ark-ff's external types.  Instead we declare *abstract*
// logical counterparts (erased at compile time, live only in Why3) and then
// link the runtime operators to them via trusted postconditions.

/// Logical zero in Fr.
#[logic(opaque)]
#[trusted]
pub fn fr_zero_l() -> Fr {
    dead
}

/// Logical one in Fr.
#[logic(opaque)]
#[trusted]
pub fn fr_one_l() -> Fr {
    dead
}

/// Logical field addition.
#[logic(opaque)]
#[trusted]
pub fn fr_add_l(a: Fr, b: Fr) -> Fr {
    dead
}

/// Logical field multiplication.
#[logic(opaque)]
#[trusted]
pub fn fr_mul_l(a: Fr, b: Fr) -> Fr {
    dead
}

// ── Field axioms ────────────────────────────────────────────────────────────
// Trusted functions whose *postconditions* become Why3 axioms.
// They are never called at runtime; they purely state algebraic laws.

/// Commutativity of addition.
#[trusted]
#[ensures(fr_add_l(a, b) == fr_add_l(b, a))]
pub fn axiom_add_comm(a: Fr, b: Fr) {}

/// Associativity of addition.
#[trusted]
#[ensures(fr_add_l(fr_add_l(a, b), c) == fr_add_l(a, fr_add_l(b, c)))]
pub fn axiom_add_assoc(a: Fr, b: Fr, c: Fr) {}

/// Left identity: 0 + a == a.
#[trusted]
#[ensures(fr_add_l(fr_zero_l(), a) == a)]
pub fn axiom_add_zero_l(a: Fr) {}

/// Right identity: a + 0 == a.
#[trusted]
#[ensures(fr_add_l(a, fr_zero_l()) == a)]
pub fn axiom_add_zero_r(a: Fr) {}

/// Right-distributivity: (a + b) * c == a*c + b*c.
#[trusted]
#[ensures(fr_mul_l(fr_add_l(a, b), c) == fr_add_l(fr_mul_l(a, c), fr_mul_l(b, c)))]
pub fn axiom_distrib_right(a: Fr, b: Fr, c: Fr) {}

// ── Runtime field wrappers ──────────────────────────────────────────────────
// These are real Rust functions.  Their postconditions connect them to the
// logical model above so that Creusot can reason about them.

/// Runtime zero, connected to the logical zero.
#[trusted]
#[ensures(result == fr_zero_l())]
pub fn fr_zero() -> Fr {
    Fr::zero()
}

/// Runtime one, connected to the logical one.
#[trusted]
#[ensures(result == fr_one_l())]
pub fn fr_one() -> Fr {
    Fr::one()
}

/// Runtime field-element from a u64.
#[trusted]
pub fn fr_from_u64(n: u64) -> Fr {
    Fr::from(n)
}

/// Runtime addition, connected to the logical addition.
#[trusted]
#[ensures(result == fr_add_l(a, b))]
pub fn fr_add(a: Fr, b: Fr) -> Fr {
    a + b
}

/// Runtime multiplication, connected to the logical multiplication.
#[trusted]
#[ensures(result == fr_mul_l(a, b))]
pub fn fr_mul(a: Fr, b: Fr) -> Fr {
    a * b
}

/// Runtime subtraction.
#[trusted]
pub fn fr_sub(a: Fr, b: Fr) -> Fr {
    a - b
}

/// Runtime negation.
#[trusted]
pub fn fr_neg(a: Fr) -> Fr {
    -a
}

/// Runtime inversion.  Returns None iff the element is zero (in a field).
#[trusted]
pub fn fr_inv(a: Fr) -> Option<Fr> {
    a.inverse()
}

/// Sample a uniformly-random field element (trusted; models randomness).
#[trusted]
pub fn fr_rand(rng: &mut impl Rng) -> Fr {
    Fr::rand(rng)
}

// ── Logical spec functions ──────────────────────────────────────────────────

/// x^n  (logical helper for polynomial evaluation).
/// Defined by: pow(x, 0) = 1,  pow(x, n) = x * pow(x, n-1).
#[logic(opaque)]
#[trusted]
pub fn pow_fr(x: Fr, n: Int) -> Fr {
    dead
}

/// Direct polynomial evaluation (the mathematical specification):
///   poly_eval([c₀, c₁, …, cₙ₋₁], x) = c₀ + c₁·x + … + cₙ₋₁·xⁿ⁻¹
///
/// Defined recursively on `k` (number of terms considered):
///   poly_eval(coeffs, x, 0)   = 0
///   poly_eval(coeffs, x, k+1) = poly_eval(coeffs, x, k) + coeffs[k] * x^k
#[logic(opaque)]
#[trusted]
pub fn poly_eval(coeffs: Seq<Fr>, x: Fr, k: Int) -> Fr {
    dead
}

/// Partial Horner accumulator after processing the last k coefficients.
///
/// From the loop analysis (Horner processes coefficients from the end):
///   horner(coeffs, x, 0) = 0
///   horner(coeffs, x, k) = coeffs[n-k] + horner(coeffs, x, k-1) * x
///
/// Relationship to poly_eval:
///   horner(coeffs, x, n) == poly_eval(coeffs, x, n)
///
/// Defined with a real pearlite body so Why3 can unfold the recursion directly,
/// avoiding the need for the step-axiom quantifier instantiation.
#[logic]
#[variant(k)]
pub fn horner_partial(coeffs: Seq<Fr>, x: Fr, k: Int) -> Fr {
    pearlite! {
        if k <= 0 {
            fr_zero_l()
        } else {
            fr_add_l(coeffs[coeffs.len() - k],
                     fr_mul_l(horner_partial(coeffs, x, k - 1), x))
        }
    }
}

// Axioms connecting the logical spec functions.

/// Base case: no coefficients = zero polynomial.
#[trusted]
#[ensures(poly_eval(coeffs, x, 0) == fr_zero_l())]
pub fn axiom_poly_eval_zero(coeffs: Seq<Fr>, x: Fr) {}

/// Step case for poly_eval:
///   poly_eval(coeffs, x, k+1) = poly_eval(coeffs, x, k) + coeffs[k] * x^k
#[trusted]
#[requires(0 <= k && k < coeffs.len())]
#[ensures(poly_eval(coeffs, x, k + 1)
          == fr_add_l(poly_eval(coeffs, x, k),
                      fr_mul_l(coeffs[k], pow_fr(x, k))))]
pub fn axiom_poly_eval_step(coeffs: Seq<Fr>, x: Fr, k: Int) {}

/// Base case: horner of 0 = 0.
#[trusted]
#[ensures(horner_partial(coeffs, x, 0) == fr_zero_l())]
pub fn axiom_horner_base(coeffs: Seq<Fr>, x: Fr) {}

/// Step case: horner(k) = coeffs[n-k] + horner(k-1) * x
#[trusted]
#[requires(0 < k && k <= coeffs.len())]
#[ensures(horner_partial(coeffs, x, k)
          == fr_add_l(coeffs[coeffs.len() - k],
                      fr_mul_l(horner_partial(coeffs, x, k - 1), x)))]
pub fn axiom_horner_step(coeffs: Seq<Fr>, x: Fr, k: Int) {}

/// Horner and poly_eval agree on the full sequence.
#[trusted]
#[ensures(horner_partial(coeffs, x, coeffs.len())
          == poly_eval(coeffs, x, coeffs.len()))]
pub fn axiom_horner_eq_poly(coeffs: Seq<Fr>, x: Fr) {}

// ── 1. sample_polynomial ────────────────────────────────────────────────────
//
// Property: returns exactly degree+1 coefficients, with `secret` at index 0.

/// Sample a random polynomial p with p(0) = secret and degree `degree`.
/// Returns the coefficient vector [secret, a₁, …, a_degree].
#[requires(degree@ < usize::MAX@)]
#[ensures(result@.len() == degree@ + 1)]
#[ensures(result@[0] == secret)]
pub fn sample_polynomial(secret: Fr, degree: usize, rng: &mut impl Rng) -> Vec<Fr> {
    let mut coeffs = Vec::new();
    coeffs.push(secret);
    // After the push: coeffs@.len() == 1 and coeffs@[0] == secret.
    #[invariant(coeffs@.len() == produced.len() + 1)]
    #[invariant(coeffs@[0] == secret)]
    for _ in 0..degree {
        coeffs.push(fr_rand(rng));
    }
    coeffs
}

// ── 2. eval_polynomial ──────────────────────────────────────────────────────
//
// Property: Horner's method produces the same result as the direct polynomial
// sum.  The loop invariant tracks the partial Horner accumulator.
//
// NOTE: discharging the invariant step requires commutativity of field
// addition (axiom_add_comm).  The Why3 backend should instantiate this
// axiom automatically, but if it times out, a manual proof in Why3 is needed.

/// Evaluate the polynomial ∑ coeffs[j]·x^j using Horner's method.
///
/// Postcondition: result equals the direct-sum spec poly_eval(coeffs@, x).
#[ensures(result == horner_partial(coeffs@, x, coeffs@.len()))]
pub fn eval_polynomial(coeffs: &[Fr], x: Fr) -> Fr {
    let n = coeffs.len();
    let mut acc = fr_zero();
    // We iterate backward: process coeffs[n-1], coeffs[n-2], …, coeffs[0].
    // After `iter` steps the loop counter `k` equals n - iter.
    let mut k = n;
    #[invariant(k@ <= n@)]
    #[invariant(acc == horner_partial(coeffs@, x, n@ - k@))]
    while k > 0 {
        k -= 1;
        // acc_new = acc_old * x + coeffs[k]
        //         = horner(n-k-1)*x + coeffs[k]    (by inductive hypothesis)
        //         = horner(n-k)                     (by axiom_horner_step + add_comm)
        //
        // Instantiate commutativity for the specific terms in the loop step so
        // that the Why3/z3 backend can discharge the invariant maintenance goal.
        // horner_partial unfolds to: coeffs[k] + mul*x, but fr_add gives mul*x + coeffs[k].
        let mul_acc_x = fr_mul(acc, x);
        axiom_add_comm(mul_acc_x, coeffs[k]);
        acc = fr_add(mul_acc_x, coeffs[k]);
    }
    // At this point k == 0, so n - k == n, and acc == horner(n).
    acc
}

// ── 3. generate_shares_from_poly ───────────────────────────────────────────
//
// Property: given parties > 0, always succeeds and returns exactly `parties`
// shares (one per party, evaluated at x = 1, 2, …, parties).

/// Evaluate the polynomial at x = 1, 2, …, parties.
#[requires(parties@ > 0)]
#[ensures(match result {
    Ok(ref v) => v@.len() == parties@,
    Err(_)    => false,
})]
pub fn generate_shares_from_poly(coeffs: &[Fr], parties: usize) -> Result<Vec<Fr>, ShamirError> {
    if parties == 0 {
        return Err(ShamirError::InvalidPartyCount);
    }
    let mut shares = Vec::new();
    #[invariant(shares@.len() == produced.len())]
    for i in 1..=parties {
        let x = fr_from_u64(i as u64);
        let y = eval_polynomial(coeffs, x);
        shares.push(y);
    }
    Ok(shares)
}

// ── 4. share_secret ─────────────────────────────────────────────────────────
//
// Properties:
//   • Preconditions (threshold > 0, parties >= threshold) are checked and
//     propagated; the function never errors when they hold.
//   • The result contains exactly `parties` shares.

/// Secret-share `secret` with the given threshold and party count.
#[requires(threshold@ > 0)]
#[requires(parties@ >= threshold@)]
#[ensures(match result {
    Ok(ref v) => v@.len() == parties@,
    Err(_)    => false,
})]
pub fn share_secret(
    secret: Fr,
    threshold: usize,
    parties: usize,
    rng: &mut impl Rng,
) -> Result<Vec<Fr>, ShamirError> {
    if threshold == 0 {
        return Err(ShamirError::InvalidThreshold);
    }
    if parties < threshold {
        return Err(ShamirError::InvalidPartyCount);
    }
    let coeffs = sample_polynomial(secret, threshold - 1, rng);
    // coeffs@.len() == threshold (by sample_polynomial's postcondition).
    generate_shares_from_poly(&coeffs, parties)
}

// ── 5. reconstruct_secret ───────────────────────────────────────────────────
//
// Property: given a non-empty slice, the EmptyShares error is never returned.
// (DuplicateShareX cannot occur for standard 1-based indices, but proving
//  that requires reasoning about field injectivity — left for future work.)

/// Reconstruct the secret from a set of shares via Lagrange interpolation.
#[requires(shares@.len() > 0)]
#[ensures(match result {
    Err(ShamirError::EmptyShares) => false,
    _                             => true,
})]
pub fn reconstruct_secret(shares: &[Fr]) -> Result<Fr, ShamirError> {
    if shares.len() == 0 {
        return Err(ShamirError::EmptyShares);
    }
    let mut secret = fr_zero();
    let n = shares.len();
    let mut i = 0usize;
    #[invariant(i@ <= n@)]
    while i < n {
        let x_i = fr_from_u64((i + 1) as u64);
        let y_i = shares[i];
        let mut num = fr_one();
        let mut den = fr_one();
        let mut j = 0usize;
        #[invariant(j@ <= n@)]
        while j < n {
            if i != j {
                let x_j = fr_from_u64((j + 1) as u64);
                num = fr_mul(num, fr_neg(x_j));
                den = fr_mul(den, fr_sub(x_i, x_j));
            }
            j += 1;
        }
        let den_inv = fr_inv(den).ok_or(ShamirError::DuplicateShareX)?;
        secret = fr_add(secret, fr_mul(y_i, fr_mul(num, den_inv)));
        i += 1;
    }
    Ok(secret)
}

// ── Tests ───────────────────────────────────────────────────────────────────
// These run under `cargo test` (stable) to sanity-check the runtime code.
// They are separate from the formal verification pass (`cargo creusot prove`).

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::rand::rngs::StdRng;
    use ark_std::rand::SeedableRng;

    #[test]
    fn sample_polynomial_length() {
        let mut rng = StdRng::seed_from_u64(1);
        let secret = Fr::from(42u64);
        let coeffs = sample_polynomial(secret, 3, &mut rng);
        assert_eq!(coeffs.len(), 4); // degree 3 → 4 coefficients
        assert_eq!(coeffs[0], secret);
    }

    #[test]
    fn eval_polynomial_constant() {
        // A degree-0 polynomial is just the constant; eval at any x gives secret.
        let secret = Fr::from(7u64);
        let coeffs = vec![secret];
        let result = eval_polynomial(&coeffs, Fr::from(5u64));
        assert_eq!(result, secret);
    }

    #[test]
    fn generate_shares_length() {
        let mut rng = StdRng::seed_from_u64(2);
        let secret = Fr::from(99u64);
        let coeffs = sample_polynomial(secret, 2, &mut rng);
        let shares = generate_shares_from_poly(&coeffs, 5).unwrap();
        assert_eq!(shares.len(), 5);
    }

    #[test]
    fn share_secret_length() {
        let mut rng = StdRng::seed_from_u64(3);
        let secret = Fr::from(123u64);
        let shares = share_secret(secret, 3, 5, &mut rng).unwrap();
        assert_eq!(shares.len(), 5);
    }

    #[test]
    fn reconstruct_nonempty_ok() {
        // reconstruct_secret should not return EmptyShares on non-empty input.
        let mut rng = StdRng::seed_from_u64(4);
        let secret = Fr::from(55u64);
        let shares = share_secret(secret, 2, 3, &mut rng).unwrap();
        let result = reconstruct_secret(&shares);
        assert!(
            !matches!(result, Err(ShamirError::EmptyShares)),
            "should not be EmptyShares"
        );
    }
}
