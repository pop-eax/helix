/// Text-level mutation operators for existing `.mpc` sample programs.
///
/// These operate directly on the source string (not the AST), which keeps
/// them simple and independent of the parser.  The primary use is to verify
/// that the pipeline never panics on mutated programs and that both oracles
/// agree on the (possibly changed) output.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mutation {
    /// Replace the first occurrence of `from` with `to`.
    SwapOp { from: &'static str, to: &'static str },
    /// Replace the first occurrence of `Public` with `Secret`.
    FlipVisibility,
    /// Replace the first bare integer literal with `0`.
    ReplaceFirstConstWithZero,
    /// Replace the first bare integer literal with `1`.
    ReplaceFirstConstWithOne,
}

/// All mutations to try against each sample file.
pub const ALL_MUTATIONS: &[Mutation] = &[
    Mutation::SwapOp { from: "+", to: "-" },
    Mutation::SwapOp { from: "-", to: "+" },
    Mutation::SwapOp { from: "*", to: "+" },
    Mutation::SwapOp { from: "+", to: "*" },
    Mutation::FlipVisibility,
    Mutation::ReplaceFirstConstWithZero,
    Mutation::ReplaceFirstConstWithOne,
];

/// Apply a single mutation to `source`.
/// Returns [None] if the pattern is not present (mutation is a no-op).
pub fn apply_mutation(source: &str, mutation: &Mutation) -> Option<String> {
    match mutation {
        Mutation::SwapOp { from, to } => {
            // Replace first occurrence of operator surrounded by spaces
            // to avoid replacing substrings inside identifiers or strings.
            let needle = format!(" {} ", from);
            let replacement = format!(" {} ", to);
            let idx = source.find(&needle)?;
            Some(format!(
                "{}{}{}",
                &source[..idx],
                replacement,
                &source[idx + needle.len()..]
            ))
        }
        Mutation::FlipVisibility => {
            if let Some(idx) = source.find("Public") {
                Some(format!("{}Secret{}", &source[..idx], &source[idx + "Public".len()..]))
            } else if let Some(idx) = source.find("Secret") {
                Some(format!("{}Public{}", &source[..idx], &source[idx + "Secret".len()..]))
            } else {
                None
            }
        }
        Mutation::ReplaceFirstConstWithZero => replace_first_integer(source, "0"),
        Mutation::ReplaceFirstConstWithOne => replace_first_integer(source, "1"),
    }
}

/// Replace the first standalone integer literal (sequence of digits) with `replacement`.
/// Skips integers that appear as type parameters (preceded by `<`, followed by `>`).
fn replace_first_integer(source: &str, replacement: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let n = bytes.len();
    for i in 0..n {
        if bytes[i].is_ascii_digit() {
            // Must not be preceded by an alphanumeric, underscore, or `<`
            // (to avoid variable names and type parameters like Field<64>)
            if i > 0
                && (bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'<')
            {
                continue;
            }
            // Find end of digit run
            let mut j = i;
            while j < n && bytes[j].is_ascii_digit() {
                j += 1;
            }
            // Must not be followed by alphanumeric, underscore, or `>` (type parameter)
            if j < n
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'>')
            {
                continue;
            }
            return Some(format!(
                "{}{}{}",
                &source[..i],
                replacement,
                &source[j..]
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_op_replaces_first_occurrence() {
        let src = "fn f(Public Field<64> a, Public Field<64> b) -> Field<64> { return a + b; }";
        let result = apply_mutation(src, &Mutation::SwapOp { from: "+", to: "-" }).unwrap();
        assert!(result.contains(" - "), "expected ' - ', got: {result}");
        assert!(!result.contains(" + "), "still contains ' + ': {result}");
    }

    #[test]
    fn flip_visibility_public_to_secret() {
        let src = "fn f(Public Field<64> a) -> Field<64> { return a; }";
        let result = apply_mutation(src, &Mutation::FlipVisibility).unwrap();
        assert!(result.contains("Secret"), "expected Secret: {result}");
    }

    #[test]
    fn replace_const_zero() {
        let src = "fn f(Public Field<64> a) -> Field<64> { return a + 42; }";
        let result = apply_mutation(src, &Mutation::ReplaceFirstConstWithZero).unwrap();
        assert!(result.contains("+ 0"), "expected 0: {result}");
    }
}
