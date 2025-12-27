use std::collections::HashMap;
use itertools::Itertools;
use runtime::executor::execute_program;
use runtime::clear::ClearBackend;
use garbledc::backend::YaoBackend;
use ir::lir::*;

fn main() {

    test_simple_and();
    test_simple_xor();

   test_addition();
   test_subtraction();
   test_multiplication();
   test_complex_expression();
}

fn build_and_program() -> Program {
    Program {
        metadata: Metadata {
            version: "1.0".to_string(),
            source_file: "test_and.mpc".to_string(),
            function_name: "and_test".to_string(),
            field_modulus: Some(256),
            statistics: Statistics {
                total_gates: 1,
                circuit_depth: 1,
                num_inputs: 2,
                num_outputs: 1,
                num_wires: 3,
                gate_counts: HashMap::new()
            },
        },
        circuit: Circuit {
            inputs: vec![
                Input {
                    wire: WireId(0),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("a".to_string()),
                },
                Input {
                    wire: WireId(1),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("b".to_string()),
                },
            ],
            gates: vec![
                Gate {
                    id: GateId(0),
                    gate_type: ir::GateType::And,
                    inputs: vec![WireId(0), WireId(1)],
                    output: WireId(2),
                },
            ],
            outputs: vec![WireId(2)],
        },
    }
}


// ============================================================================
// Program Builders
// ============================================================================

fn build_xor_program() -> Program {
    Program {
        metadata: Metadata {
            version: "1.0".to_string(),
            source_file: "test_xor.mpc".to_string(),
            function_name: "xor_test".to_string(),
            field_modulus: Some(256),
            statistics: Statistics {
                total_gates: 1,
                circuit_depth: 1,
                num_inputs: 2,
                num_outputs: 1,
                num_wires: 3,
                gate_counts: HashMap::new()
            },
        },
        circuit: Circuit {
            inputs: vec![
                Input {
                    wire: WireId(0),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("a".to_string()),
                },
                Input {
                    wire: WireId(1),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("b".to_string()),
                },
            ],
            gates: vec![
                Gate {
                    id: GateId(0),
                    gate_type: ir::GateType::Xor,
                    inputs: vec![WireId(0), WireId(1)],
                    output: WireId(2),
                },
            ],
            outputs: vec![WireId(2)],
        },
    }
}

fn build_add_program() -> Program {
    Program {
        metadata: Metadata {
            version: "1.0".to_string(),
            source_file: "test_add.mpc".to_string(),
            function_name: "add_test".to_string(),
            field_modulus: Some(256),
            statistics: Statistics {
                total_gates: 1,
                circuit_depth: 1,
                num_inputs: 2,
                num_outputs: 1,
                num_wires: 3,
                gate_counts: HashMap::new()
            },
        },
        circuit: Circuit {
            inputs: vec![
                Input {
                    wire: WireId(0),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("a".to_string()),
                },
                Input {
                    wire: WireId(1),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("b".to_string()),
                },
            ],
            gates: vec![
                Gate {
                    id: GateId(0),
                    gate_type: GateType::Add,
                    inputs: vec![WireId(0), WireId(1)],
                    output: WireId(2),
                },
            ],
            outputs: vec![WireId(2)],
        },
    }
}

fn build_sub_program() -> Program {
    Program {
        metadata: Metadata {
            version: "1.0".to_string(),
            source_file: "test_sub.mpc".to_string(),
            function_name: "sub_test".to_string(),
            field_modulus: Some(256),
            statistics: Statistics {
                total_gates: 1,
                circuit_depth: 1,
                num_inputs: 2,
                num_outputs: 1,
                num_wires: 3,
                gate_counts: HashMap::new()
            },
        },
        circuit: Circuit {
            inputs: vec![
                Input {
                    wire: WireId(0),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("a".to_string()),
                },
                Input {
                    wire: WireId(1),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("b".to_string()),
                },
            ],
            gates: vec![
                Gate {
                    id: GateId(0),
                    gate_type: ir::GateType::Sub,
                    inputs: vec![WireId(0), WireId(1)],
                    output: WireId(2),
                },
            ],
            outputs: vec![WireId(2)],
        },
    }
}

fn build_mul_program() -> Program {
    Program {
        metadata: Metadata {
            version: "1.0".to_string(),
            source_file: "test_mul.mpc".to_string(),
            function_name: "mul_test".to_string(),
            field_modulus: Some(256),
            statistics: Statistics {
                total_gates: 1,
                circuit_depth: 1,
                num_inputs: 2,
                num_outputs: 1,
                num_wires: 3,
                gate_counts: HashMap::new()
                
            },
        },
        circuit: Circuit {
            inputs: vec![
                Input {
                    wire: WireId(0),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("a".to_string()),
                },
                Input {
                    wire: WireId(1),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("b".to_string()),
                },
            ],
            gates: vec![
                Gate {
                    id: GateId(0),
                    gate_type: ir::GateType::Mul,
                    inputs: vec![WireId(0), WireId(1)],
                    output: WireId(2),
                },
            ],
            outputs: vec![WireId(2)],
        },
    }
}

fn build_complex_program() -> Program {
    // (a + b) * (c - d)
    // Wire 0: a
    // Wire 1: b
    // Wire 2: c
    // Wire 3: d
    // Wire 4: a + b
    // Wire 5: c - d
    // Wire 6: (a+b) * (c-d)
    
    Program {
        metadata: Metadata {
            version: "1.0".to_string(),
            source_file: "test_complex.mpc".to_string(),
            function_name: "complex_test".to_string(),
            field_modulus: Some(256),
            statistics: Statistics {
                total_gates: 3,
                circuit_depth: 2,
                num_inputs: 4,
                num_outputs: 1,
                num_wires: 7,
                gate_counts: HashMap::new()
            },
        },
        circuit: Circuit {
            inputs: vec![
                Input {
                    wire: WireId(0),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("a".to_string()),
                },
                Input {
                    wire: WireId(1),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("b".to_string()),
                },
                Input {
                    wire: WireId(2),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("c".to_string()),
                },
                Input {
                    wire: WireId(3),
                    party: None,
                    visibility: Visibility::Secret,
                    name: Some("d".to_string()),
                },
            ],
            gates: vec![
                Gate {
                    id: GateId(0),
                    gate_type: ir::GateType::Add,
                    inputs: vec![WireId(0), WireId(1)],
                    output: WireId(4),
                },
                Gate {
                    id: GateId(1),
                    gate_type: ir::GateType::Sub,
                    inputs: vec![WireId(2), WireId(3)],
                    output: WireId(5),
                },
                Gate {
                    id: GateId(2),
                    gate_type: ir::GateType::Mul,
                    inputs: vec![WireId(4), WireId(5)],
                    output: WireId(6),
                },
            ],
            outputs: vec![WireId(6)],
        },
    }
}

fn test_simple_and() {
    println!("Test 1: AND Gate");
    println!("----------------");
    
    let program = build_and_program();
    
    // Test cases: (a, b, expected)
    let test_cases = vec![
        (0, 0, 0),
        (0, 1, 0),
        (1, 0, 0),
        (1, 1, 1),
    ];
    
    for (a, b, expected) in test_cases {
        // Test with Clear backend
        let mut clear = ClearBackend::new(None);
        let clear_result = execute_program(
            &program,
            &mut clear,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap();
        
        // Test with Yao backend
        let mut yao = YaoBackend::new(8);
        let yao_result = execute_program(
            &program,
            &mut yao,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap();
        
        let clear_output = clear_result[0].1;
        let yao_output = yao_result[0].1;
        
        println!("  {} AND {} = {} (Clear: {}, Yao: {})", 
            a, b, expected, clear_output, yao_output);
        
        assert_eq!(clear_output, expected, "Clear backend failed");
        assert_eq!(yao_output, expected, "Yao backend failed");
        assert_eq!(clear_output, yao_output, "Backends disagree!");
    }
    
    println!("  ✓ AND gate working\n");
}

fn test_simple_xor() {
    println!("Test 2: XOR Gate (Free!)");
    println!("------------------------");
    
    let program = build_xor_program();
    
    let test_cases = vec![
        (0, 0, 0),
        (0, 1, 1),
        (1, 0, 1),
        (1, 1, 0),
    ];
    
    for (a, b, expected) in test_cases {
        let mut clear = ClearBackend::new(None);
        let mut yao = YaoBackend::new(8);
        
        let clear_result = execute_program(
            &program, &mut clear,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap()[0].1;
        
        let yao_result = execute_program(
            &program, &mut yao,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap()[0].1;
        
        println!("  {} XOR {} = {} (Clear: {}, Yao: {})", 
            a, b, expected, clear_result, yao_result);
        
        assert_eq!(clear_result, expected);
        assert_eq!(yao_result, expected);
    }
    
    println!("  ✓ XOR gate working (no garbled tables needed!)\n");
}

/// Test 3: Addition
fn test_addition() {
    println!("Test 3: 8-bit Addition");
    println!("----------------------");
    
    let program = build_add_program();
    
    let test_cases = vec![
        (0, 0, 0),
        (1, 1, 2),
        (42, 17, 59),
        (100, 55, 155),
        (200, 100, 44),  // Overflow: wraps to 44 in 8-bit
        (255, 1, 0),     // Overflow: wraps to 0
    ];
    
    for (a, b, expected) in test_cases {
        let mut clear = ClearBackend::new(Some(256));
        let mut yao = YaoBackend::new(8);
        
        let clear_result = execute_program(
            &program, &mut clear,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap()[0].1;
        
        let yao_result = execute_program(
            &program, &mut yao,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap()[0].1;
        
        println!("  {} + {} = {} (Clear: {}, Yao: {})", 
            a, b, expected, clear_result, yao_result);
        
        assert_eq!(clear_result, expected);
        assert_eq!(yao_result, expected);
    }
    
    println!("  ✓ Addition working\n");
}

/// Test 4: Subtraction
fn test_subtraction() {
    println!("Test 4: 8-bit Subtraction");
    println!("-------------------------");
    
    let program = build_sub_program();
    
    let test_cases = vec![
        (5, 3, 2),
        (10, 10, 0),
        (100, 42, 58),
        (3, 5, 254),  // Underflow in 8-bit: 3 - 5 = -2 = 254
        (0, 1, 255),  // Underflow: 0 - 1 = -1 = 255
    ];
    
    for (a, b, expected) in test_cases {
        let mut clear = ClearBackend::new(Some(256));
        let mut yao = YaoBackend::new(8);
        
        let clear_result = execute_program(
            &program, &mut clear,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap()[0].1;
        
        let yao_result = execute_program(
            &program, &mut yao,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap()[0].1;
        
        println!("  {} - {} = {} (Clear: {}, Yao: {})", 
            a, b, expected, clear_result, yao_result);
        
        assert_eq!(clear_result, expected);
        assert_eq!(yao_result, expected);
    }
    
    println!("  ✓ Subtraction working\n");
}

/// Test 5: Multiplication
fn test_multiplication() {
    println!("Test 5: 8-bit Multiplication");
    println!("----------------------------");
    
    let program = build_mul_program();
    
    let test_cases = vec![
        (0, 0, 0),
        (1, 1, 1),
        (5, 3, 15),
        (10, 10, 100),
        (15, 15, 225),
        (16, 16, 0),   // Overflow: 256 % 256 = 0 in 8-bit
        (20, 13, 4),   // Overflow: 260 % 256 = 4
    ];
    
    for (a, b, expected) in test_cases {
        let mut clear = ClearBackend::new(Some(256));
        let mut yao = YaoBackend::new(8);
        
        let clear_result = execute_program(
            &program, &mut clear,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap()[0].1;
        
        let yao_result = execute_program(
            &program, &mut yao,
            &[(WireId(0), PartyId(0), a), (WireId(1), PartyId(1), b)],
        ).unwrap()[0].1;
        
        println!("  {} * {} = {} (Clear: {}, Yao: {})", 
            a, b, expected, clear_result, yao_result);
        
        assert_eq!(clear_result, expected);
        assert_eq!(yao_result, expected);
    }
    
    println!("  ✓ Multiplication working\n");
}

/// Test 6: Complex expression
fn test_complex_expression() {
    println!("Test 6: Complex Expression");
    println!("--------------------------");
    println!("Computing: (a + b) * (c - d)");
    
    let program = build_complex_program();
    
    let test_cases = vec![
        (5, 3, 10, 2, 64),   // (5+3) * (10-2) = 8 * 8 = 64
        (10, 5, 8, 3, 75),   // (10+5) * (8-3) = 15 * 5 = 75
        (2, 3, 7, 4, 15),    // (2+3) * (7-4) = 5 * 3 = 15
    ];
    
    for (a, b, c, d, expected) in test_cases {
        let mut clear = ClearBackend::new(Some(256));
        let mut yao = YaoBackend::new(8);
        
        let clear_result = execute_program(
            &program, &mut clear,
            &[
                (WireId(0), PartyId(0), a),
                (WireId(1), PartyId(1), b),
                (WireId(2), PartyId(0), c),
                (WireId(3), PartyId(1), d),
            ],
        ).unwrap()[0].1;
        
        let yao_result = execute_program(
            &program, &mut yao,
            &[
                (WireId(0), PartyId(0), a),
                (WireId(1), PartyId(1), b),
                (WireId(2), PartyId(0), c),
                (WireId(3), PartyId(1), d),
            ],
        ).unwrap()[0].1;
        
        println!("  ({} + {}) * ({} - {}) = {} (Clear: {}, Yao: {})", 
            a, b, c, d, expected, clear_result, yao_result);
        
        assert_eq!(clear_result, expected);
        assert_eq!(yao_result, expected);
    }
    
    println!("  ✓ Complex expression working\n");
}

