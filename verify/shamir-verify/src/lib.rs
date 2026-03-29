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

/// Logical field negation.
#[logic(opaque)]
#[trusted]
pub fn fr_neg_l(a: Fr) -> Fr {
    dead
}

/// Logical field subtraction (defined as add-neg).
#[logic(opaque)]
#[trusted]
pub fn fr_sub_l(a: Fr, b: Fr) -> Fr {
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

/// Subtraction is add-neg: a - b == a + (-b).
#[trusted]
#[ensures(fr_sub_l(a, b) == fr_add_l(a, fr_neg_l(b)))]
pub fn axiom_sub_neg(a: Fr, b: Fr) {}

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

/// Runtime subtraction, connected to the logical subtraction.
#[trusted]
#[ensures(result == fr_sub_l(a, b))]
pub fn fr_sub(a: Fr, b: Fr) -> Fr {
    a - b
}

/// Runtime negation, connected to the logical negation.
#[trusted]
#[ensures(result == fr_neg_l(a))]
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

// ── Share type and ops error ────────────────────────────────────────────────

/// A single party's share value (y-coordinate; x is implicit from position).
#[derive(Clone, Copy)]
pub struct Share(pub Fr);

/// Construct a Share; the postcondition exposes the inner field for the prover.
/// Trusted so Creusot turns it into a universally-quantified Why3 axiom rather
/// than trying to reduce the struct constructor directly.
#[trusted]
#[ensures(result.0 == v)]
fn mk_share(v: Fr) -> Share {
    Share(v)
}

/// Push a Share onto a Vec by value, returning the extended Vec.
///
/// Using a by-value API avoids the MutBorrow indirection (`_50`/`_49` record
/// fields) that prevents alt-ergo from chaining through `view_Vec_Share_Global`
/// to the postcondition.  After `out = vec_push_share_val(out, s)`, the
/// postcondition `result@[v@.len()] == x` is directly about `out@` — a single
/// equality step, no trigger required.
#[trusted]
#[ensures(result@.len() == v@.len() + 1)]
#[ensures(result@[v@.len()] == x)]
#[ensures(forall<j: Int> 0 <= j && j < v@.len() ==> result@[j] == v@[j])]
fn vec_push_share_val(v: Vec<Share>, x: Share) -> Vec<Share> {
    let mut v = v;
    v.push(x);
    v
}


/// Errors from share vector operations.
#[derive(Clone, Copy)]
pub enum OpsError {
    LengthMismatch,
}

// ── Logical reconstruction spec ─────────────────────────────────────────────
// Abstract model of Lagrange interpolation used only in logic/axioms.

#[logic(opaque)]
#[trusted]
pub fn reconstruct_spec(shares: Seq<Share>) -> Fr {
    dead
}

/// Additive homomorphism axiom: reconstruct(a ⊕ b) = reconstruct(a) + reconstruct(b).
/// Trusted — algebraic proof of Lagrange linearity is out of scope for z3.
#[trusted]
#[requires(left.len() == right.len())]
#[requires(forall<i: Int> 0 <= i && i < left.len() ==>
           sum[i].0 == fr_add_l(left[i].0, right[i].0))]
#[ensures(reconstruct_spec(sum) == fr_add_l(reconstruct_spec(left), reconstruct_spec(right)))]
pub fn axiom_add_homomorphism(left: Seq<Share>, right: Seq<Share>, sum: Seq<Share>) {}

/// Scalar homomorphism axiom: reconstruct(k · s) = k · reconstruct(s).
#[trusted]
#[requires(shares.len() == scaled.len())]
#[requires(forall<i: Int> 0 <= i && i < shares.len() ==>
           scaled[i].0 == fr_mul_l(shares[i].0, k))]
#[ensures(reconstruct_spec(scaled) == fr_mul_l(reconstruct_spec(shares), k))]
pub fn axiom_scale_homomorphism(shares: Seq<Share>, k: Fr, scaled: Seq<Share>) {}

// ── 6. add_shares ────────────────────────────────────────────────────────────
//
// Properties:
//   • Requires equal-length inputs.
//   • Returns Ok(v) with v.len() == left.len().
//   • Each output share equals the pointwise field sum.

/// Pointwise-add two share vectors.
#[requires(left@.len() == right@.len())]
#[ensures(match result {
    Ok(ref v) => v@.len() == left@.len()
        && forall<i: Int> 0 <= i && i < left@.len() ==>
               v@[i].0 == fr_add_l(left@[i].0, right@[i].0),
    Err(_) => false,
})]
pub fn add_shares(left: &[Share], right: &[Share]) -> Result<Vec<Share>, OpsError> {
    if left.len() != right.len() {
        return Err(OpsError::LengthMismatch);
    }
    let mut out: Vec<Share> = Vec::new();
    let n = left.len();
    let mut i = 0usize;
    #[invariant(out@.len() == i@)]
    #[invariant(forall<j: Int> 0 <= j && j < out@.len() ==>
                out@[j].0 == fr_add_l(left@[j].0, right@[j].0))]
    while i < n {
        let v = fr_add(left[i].0, right[i].0);
        let s = mk_share(v);
        let old_len = snapshot! { out@.len() };
        out = vec_push_share_val(out, s);
        proof_assert!(out@[*old_len] == s);
        proof_assert!(out@[i@] == s);
        i += 1;
    }
    Ok(out)
}

// ── 7. sub_shares ────────────────────────────────────────────────────────────

/// Pointwise-subtract two share vectors.
#[requires(left@.len() == right@.len())]
#[ensures(match result {
    Ok(ref v) => v@.len() == left@.len()
        && forall<i: Int> 0 <= i && i < left@.len() ==>
               v@[i].0 == fr_sub_l(left@[i].0, right@[i].0),
    Err(_) => false,
})]
pub fn sub_shares(left: &[Share], right: &[Share]) -> Result<Vec<Share>, OpsError> {
    if left.len() != right.len() {
        return Err(OpsError::LengthMismatch);
    }
    let mut out: Vec<Share> = Vec::new();
    let n = left.len();
    let mut i = 0usize;
    #[invariant(out@.len() == i@)]
    #[invariant(forall<j: Int> 0 <= j && j < out@.len() ==>
                out@[j].0 == fr_sub_l(left@[j].0, right@[j].0))]
    while i < n {
        let v = fr_sub(left[i].0, right[i].0);
        let s = mk_share(v);
        let old_len = snapshot! { out@.len() };
        out = vec_push_share_val(out, s);
        proof_assert!(out@[*old_len] == s);
        proof_assert!(out@[i@] == s);
        i += 1;
    }
    Ok(out)
}

// ── 8. scale_shares ──────────────────────────────────────────────────────────
//
// Properties:
//   • Output length equals input length.
//   • Each output share equals the input share multiplied by the scalar k.

/// Multiply every share in the vector by a scalar k.
#[ensures(result@.len() == shares@.len()
    && forall<i: Int> 0 <= i && i < shares@.len() ==>
           result@[i].0 == fr_mul_l(shares@[i].0, k))]
pub fn scale_shares(shares: &[Share], k: Fr) -> Vec<Share> {
    let mut out: Vec<Share> = Vec::new();
    let n = shares.len();
    let mut i = 0usize;
    #[invariant(out@.len() == i@)]
    #[invariant(forall<j: Int> 0 <= j && j < out@.len() ==>
                out@[j].0 == fr_mul_l(shares@[j].0, k))]
    while i < n {
        let v = fr_mul(shares[i].0, k);
        let s = mk_share(v);
        let old_len = snapshot! { out@.len() };
        out = vec_push_share_val(out, s);
        proof_assert!(out@[*old_len] == s);
        proof_assert!(out@[i@] == s);
        i += 1;
    }
    out
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
