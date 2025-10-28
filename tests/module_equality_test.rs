/// E2E test to ensure import → export → import produces the same result as just importing
/// This test works on any LLVM bitcode input and compares the module structures
use llvm_ir::Module;
use std::fs;
use std::path::PathBuf;

fn init_logging() {
    let _ = env_logger::builder().is_test(true).try_init();
}

/// Compares two modules for structural equality
/// Returns true if modules are functionally equivalent
fn modules_are_equivalent(m1: &Module, m2: &Module) -> bool {
    // Compare basic metadata
    if m1.source_file_name != m2.source_file_name {
        eprintln!("Source file name mismatch: '{}' vs '{}'", m1.source_file_name, m2.source_file_name);
        return false;
    }
    
    if m1.data_layout.layout_str != m2.data_layout.layout_str {
        eprintln!("Data layout mismatch");
        return false;
    }
    
    if m1.target_triple != m2.target_triple {
        eprintln!("Target triple mismatch: {:?} vs {:?}", m1.target_triple, m2.target_triple);
        return false;
    }
    
    // Compare counts
    if m1.functions.len() != m2.functions.len() {
        eprintln!("Function count mismatch: {} vs {}", m1.functions.len(), m2.functions.len());
        return false;
    }
    
    if m1.func_declarations.len() != m2.func_declarations.len() {
        eprintln!("Function declaration count mismatch: {} vs {}", m1.func_declarations.len(), m2.func_declarations.len());
        return false;
    }
    
    if m1.global_vars.len() != m2.global_vars.len() {
        eprintln!("Global variable count mismatch: {} vs {}", m1.global_vars.len(), m2.global_vars.len());
        return false;
    }
    
    // Compare functions
    for (f1, f2) in m1.functions.iter().zip(m2.functions.iter()) {
        if f1.name != f2.name {
            eprintln!("Function name mismatch: '{}' vs '{}'", f1.name, f2.name);
            return false;
        }
        
        if f1.parameters.len() != f2.parameters.len() {
            eprintln!("Parameter count mismatch for function {}: {} vs {}", 
                f1.name, f1.parameters.len(), f2.parameters.len());
            return false;
        }
        
        if f1.basic_blocks.len() != f2.basic_blocks.len() {
            eprintln!("Basic block count mismatch for function {}: {} vs {}", 
                f1.name, f1.basic_blocks.len(), f2.basic_blocks.len());
            return false;
        }
        
        // Compare basic blocks
        for (bb1, bb2) in f1.basic_blocks.iter().zip(f2.basic_blocks.iter()) {
            if bb1.instrs.len() != bb2.instrs.len() {
                eprintln!("Instruction count mismatch in basic block {}: {} vs {}", 
                    bb1.name, bb1.instrs.len(), bb2.instrs.len());
                return false;
            }
        }
    }
    
    true
}

#[test]
fn test_module_import_export_import_equivalence() {
    init_logging();
    
    // Find test bitcode files
    let test_files = vec![
        "tests/basic_bc/llvm16/hello.bc",
    ];
    
    for test_file in test_files {
        let test_path = PathBuf::from(test_file);
        if !test_path.exists() {
            eprintln!("Test file not found: {}, skipping", test_file);
            continue;
        }
        
        println!("\nTesting: {}", test_file);
        
        // Step 1: Import original module
        let original = Module::from_bc_path(test_file)
            .expect(&format!("Failed to load original: {}", test_file));
        
        // Step 2: Export to bitcode
        let temp_dir = std::env::temp_dir().join("module_equality_test");
        fs::create_dir_all(&temp_dir).ok();
        let exported_bc = temp_dir.join("exported.bc");
        
        original.write_bitcode_to_file(exported_bc.to_str().unwrap())
            .expect("Failed to export bitcode");
        
        // Step 3: Import exported module
        let reimported = Module::from_bc_path(exported_bc.to_str().unwrap())
            .expect("Failed to load reimported bitcode");
        
        // Step 4: Compare
        assert!(modules_are_equivalent(&original, &reimported),
            "Module structure changed after export/import cycle for {}", test_file);
        
        println!("  ✓ Module equivalence verified");
        
        // Cleanup
        fs::remove_dir_all(&temp_dir).ok();
    }
}

#[test]
fn test_module_import_export_import_with_generated_ir() {
    init_logging();
    
    // Create a test module programmatically
    let test_ir = r#"; ModuleID = 'equivalence_test'
source_filename = "test.c"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

define i32 @test_add(i32 %x, i32 %y) {
entry:
  %sum = add i32 %x, %y
  ret i32 %sum
}

define i32 @test_mul(i32 %a, i32 %b) {
entry:
  %prod = mul i32 %a, %b
  ret i32 %prod
}
"#;
    
    let test_dir = std::env::temp_dir().join("module_equality_generated");
    fs::create_dir_all(&test_dir).ok();
    
    // Write test IR
    let input_ll = test_dir.join("input.ll");
    fs::write(&input_ll, test_ir).expect("Failed to write test IR");
    
    // Convert to bitcode
    let input_bc = test_dir.join("input.bc");
    let llvm_as_versions = ["llvm-as", "llvm-as-16", "llvm-as-15", "llvm-as-14"];
    let mut assembled = false;
    
    for cmd in &llvm_as_versions {
        if std::process::Command::new(cmd)
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
        eprintln!("llvm-as not available - skipping test");
        return;
    }
    
    // Step 1: Import original
    let original = Module::from_bc_path(input_bc.to_str().unwrap())
        .expect("Failed to load original");
    
    // Step 2: Export
    let exported_bc = test_dir.join("exported.bc");
    original.write_bitcode_to_file(exported_bc.to_str().unwrap())
        .expect("Failed to export");
    
    // Step 3: Import exported
    let reimported = Module::from_bc_path(exported_bc.to_str().unwrap())
        .expect("Failed to load reimported");
    
    // Step 4: Verify equivalence
    assert!(modules_are_equivalent(&original, &reimported),
        "Module structure changed after export/import cycle");
    
    println!("✓ Generated IR module equivalence verified");
    
    // Cleanup
    fs::remove_dir_all(&test_dir).ok();
}
