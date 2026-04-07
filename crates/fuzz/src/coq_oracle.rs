/// Coq oracle runner.
///
/// Spawns the extracted `helix_eval` OCaml binary as a subprocess, sends the
/// test case JSON over stdin, and reads the JSON result from stdout.
///
/// If the binary is not present the test is silently skipped via [CoqError::NotBuilt].
use std::io::Write;
use std::process::{Command, Stdio};

use crate::generator::TestCase;

/// Path to the compiled OCaml oracle binary, relative to the workspace root.
pub const COQ_RUNNER: &str = "verify/dsl-verify/extraction/helix_eval";

#[derive(Debug, PartialEq)]
pub enum CoqError {
    /// Binary not built yet — caller should skip the test, not fail it.
    NotBuilt,
    /// Evaluation was stuck (division by zero, out-of-scope var, etc.).
    Stuck,
    /// Unexpected error (I/O failure, malformed output, etc.).
    Other(String),
}

impl std::fmt::Display for CoqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoqError::NotBuilt => write!(f, "helix_eval not built"),
            CoqError::Stuck => write!(f, "stuck"),
            CoqError::Other(s) => write!(f, "coq oracle error: {s}"),
        }
    }
}

/// Evaluate [tc] using the Coq oracle and return the numeric result.
pub fn eval_with_coq(tc: &TestCase) -> Result<u64, CoqError> {
    // Check if binary exists; skip gracefully if not.
    if !std::path::Path::new(COQ_RUNNER).exists() {
        return Err(CoqError::NotBuilt);
    }

    let json = tc.to_json();

    let mut child = Command::new(COQ_RUNNER)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| CoqError::Other(format!("spawn: {e}")))?;

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(json.as_bytes())
        .map_err(|e| CoqError::Other(format!("write stdin: {e}")))?;

    let output = child
        .wait_with_output()
        .map_err(|e| CoqError::Other(format!("wait: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_coq_output(stdout.trim())
}

fn parse_coq_output(s: &str) -> Result<u64, CoqError> {
    // Expected: {"ok":42}  or  {"err":"stuck"}  or  {"err":"aggregate-result"}
    let v: serde_json::Value = serde_json::from_str(s)
        .map_err(|_| CoqError::Other(format!("bad JSON output: {s:?}")))?;

    if let Some(ok) = v.get("ok") {
        let n = ok
            .as_i64()
            .ok_or_else(|| CoqError::Other(format!("ok is not an int: {ok}")))?;
        Ok(n as u64)
    } else if let Some(err) = v.get("err") {
        let msg = err.as_str().unwrap_or("unknown");
        if msg == "stuck" || msg == "aggregate-result" {
            Err(CoqError::Stuck)
        } else {
            Err(CoqError::Other(msg.to_string()))
        }
    } else {
        Err(CoqError::Other(format!("unrecognised output: {s}")))
    }
}
