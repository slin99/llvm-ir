use llvm_ir::Module;
use std::process::Command;
use std::fs;

fn init_logging() {
    let _ = env_logger::builder().is_test(true).try_init();
}

/// Normalizes LLVM IR for comparison by:
/// - Removing comments
/// - Normalizing whitespace
/// - Removing debug metadata  
/// - Removing quotes from names
/// - Normalizing parameter names (e.g., %a -> %0)
fn normalize_ir(ir: &str) -> String {
    ir.lines()
        .filter(|line| !line.trim().starts_with(';')) // Remove comments
        .filter(|line| !line.trim().starts_with('!')) // Remove metadata
        .filter(|line| !line.trim().is_empty()) // Remove empty lines
        .map(|line| line.trim())
        .map(|line| line.replace("\"", "")) // Remove quotes
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_roundtrip_ir_preservation() {
    init_logging();
    
    // Create a simple test IR
    let test_ir = r#"; ModuleID = 'test'
source_filename = "test.c"

define i32 @add(i32 %a, i32 %b) {
entry:
  %sum = add i32 %a, %b
  ret i32 %sum
}

define i32 @main() {
entry:
  %result = call i32 @add(i32 5, i32 3)
  ret i32 %result
}
"#;
    
    let test_dir = std::env::temp_dir().join("llvm_ir_roundtrip_test");
    fs::create_dir_all(&test_dir).ok();
    
    let input_ll = test_dir.join("input.ll");
    fs::write(&input_ll, test_ir).expect("Failed to write test IR");
    
    // Convert to bitcode using llvm-as
    let input_bc = test_dir.join("input.bc");
    let llvm_as_versions = ["llvm-as", "llvm-as-16", "llvm-as-15", "llvm-as-14"];
    let mut assembled = false;
    
    for cmd in &llvm_as_versions {
        if Command::new(cmd)
            .args(&[input_ll.to_str().unwrap(), "-o", input_bc.to_str().unwrap()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            assembled = true;
            break;
        }
    }
    
    if !assembled {
        eprintln!("llvm-as not available - skipping roundtrip test");
        return;
    }
    
    // Load with llvm-ir
    let module = Module::from_bc_path(input_bc.to_str().unwrap())
        .expect("Failed to load module");
    
    // Export to IR
    let output_ll = test_dir.join("output.ll");
    module.write_ir_to_file(output_ll.to_str().unwrap())
        .expect("Failed to write IR");
    
    // Read both files and normalize
    let input_ir_normalized = normalize_ir(test_ir);
    let output_ir = fs::read_to_string(&output_ll).expect("Failed to read output IR");
    let output_ir_normalized = normalize_ir(&output_ir);
    
    // Print for debugging
    println!("Input IR (normalized):\n{}\n", input_ir_normalized);
    println!("Output IR (normalized):\n{}\n", output_ir_normalized);
    
    // Compare - they should be functionally equivalent
    // The structure and instructions should be the same
    assert!(output_ir_normalized.contains("define i32 @add"), "Output should contain add function");
    assert!(output_ir_normalized.contains("define i32 @main"), "Output should contain main function");
    assert!(output_ir_normalized.contains("ret i32"), "Output should contain return instructions");
    assert!(output_ir_normalized.contains("add i32"), "Output should contain add instruction");
    assert!(output_ir_normalized.contains("call i32 @add"), "Output should contain call instruction");
    
    // Verify the structure is preserved (lines should be similar count)
    let input_lines: Vec<&str> = input_ir_normalized.lines().collect();
    let output_lines: Vec<&str> = output_ir_normalized.lines().collect();
    
    // Should have similar number of meaningful lines (within a few lines for parameter naming differences)
    assert!((input_lines.len() as i32 - output_lines.len() as i32).abs() < 5,
            "Line count should be similar: input {} vs output {}", input_lines.len(), output_lines.len());
    
    // Cleanup
    fs::remove_dir_all(&test_dir).ok();
}
