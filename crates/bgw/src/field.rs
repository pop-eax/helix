use ark_ff::PrimeField;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldConversionError {
    ValueDoesNotFitU64,
}

pub fn u64_to_field<F: PrimeField>(value: u64) -> F {
    F::from(value)
}

pub fn field_to_u64_checked<F: PrimeField>(value: F) -> Result<u64, FieldConversionError> {
    let bigint = value.into_bigint();
    let limbs = bigint.as_ref();
    if limbs.len() > 1 && limbs[1..].iter().any(|&limb| limb != 0) {
        return Err(FieldConversionError::ValueDoesNotFitU64);
    }
    Ok(limbs[0])
}
