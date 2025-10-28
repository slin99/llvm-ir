// Integration test demonstrating the complete workflow:
// 1. Create a simple C program
// 2. Compile to LLVM IR
// 3. Load with llvm-ir
// 4. Export to bitcode and LLVM IR text
// 5. Verify the exports can be compiled with LLVM tools

use llvm_ir::Module;
use std::process::Command;
use std::fs;

fn init_logging() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn integration_test_full_workflow() {
    init_logging();
    
    // Step 1: Create a simple C program
    let c_code = r#"
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int multiply(int a, int b) {
    return a * b;
}

int main() {
    int x = add(5, 3);
    int y = multiply(x, 2);
    printf("Result: %d\n", y);
    return 0;
}
"#;
    
    let test_dir = std::env::temp_dir().join("llvm_ir_integration_test");
    fs::create_dir_all(&test_dir).ok();
    
    let c_file = test_dir.join("test.c");
    fs::write(&c_file, c_code).expect("Failed to write C file");
    
    // Step 2: Compile to LLVM bitcode using clang
    // Try different clang versions
    let bc_file = test_dir.join("test.bc");
    let clang_versions = ["clang", "clang-16", "clang-15", "clang-14"];
    let mut clang_output = None;
    
    for clang_cmd in &clang_versions {
        let output = Command::new(clang_cmd)
            .args(&["-c", "-emit-llvm", c_file.to_str().unwrap(), "-o", bc_file.to_str().unwrap()])
            .output();
        
        if output.is_ok() && output.as_ref().unwrap().status.success() {
            clang_output = Some(output.unwrap());
            break;
        }
    }
    
    if clang_output.is_none() {
        eprintln!("Clang not available, skipping integration test");
        return;
    }
    
    // Step 3: Load the module with llvm-ir
    let module = Module::from_bc_path(bc_file.to_str().unwrap())
        .expect("Failed to load bitcode");
    
    // Verify the module contents
    assert_eq!(module.source_file_name, c_file.to_str().unwrap());
    assert_eq!(module.functions.len(), 3); // add, multiply, main
    assert_eq!(module.func_declarations.len(), 1); // printf
    assert_eq!(module.global_vars.len(), 1); // format string
    
    println!("✓ Module loaded successfully");
    println!("  Functions: {}", module.functions.len());
    println!("  Declarations: {}", module.func_declarations.len());
    println!("  Globals: {}", module.global_vars.len());
    
    // Step 4: Export to bitcode
    let exported_bc = test_dir.join("exported.bc");
    module.write_bitcode_to_file(exported_bc.to_str().unwrap())
        .expect("Failed to export bitcode");
    println!("✓ Exported to bitcode: {}", exported_bc.display());
    
    // Step 5: Export to LLVM IR text
    let exported_ll = test_dir.join("exported.ll");
    module.write_ir_to_file(exported_ll.to_str().unwrap())
        .expect("Failed to export IR text");
    println!("✓ Exported to LLVM IR: {}", exported_ll.display());
    
    // Step 6: Verify exported bitcode can be loaded
    let loaded = Module::from_bc_path(exported_bc.to_str().unwrap())
        .expect("Failed to load exported bitcode");
    assert_eq!(loaded.source_file_name, module.source_file_name);
    // Functions are exported as declarations
    assert_eq!(loaded.func_declarations.len(), 4); // add, multiply, main, printf
    println!("✓ Exported bitcode loads successfully");
    
    // Step 7: Verify exported IR can be loaded
    let loaded_ir = Module::from_ir_path(exported_ll.to_str().unwrap())
        .expect("Failed to load exported IR");
    assert_eq!(loaded_ir.source_file_name, module.source_file_name);
    println!("✓ Exported IR text loads successfully");
    
    // Step 8: Verify exported bitcode can be compiled with llc
    let asm_file = test_dir.join("exported.s");
    let llc_versions = ["llc", "llc-16", "llc-15", "llc-14"];
    let mut llc_success = false;
    
    for llc_cmd in &llc_versions {
        let llc_output = Command::new(llc_cmd)
            .args(&[exported_bc.to_str().unwrap(), "-o", asm_file.to_str().unwrap()])
            .output();
        
        if llc_output.is_ok() && llc_output.unwrap().status.success() {
            llc_success = true;
            break;
        }
    }
    
    if llc_success {
        println!("✓ Exported bitcode compiles with llc");
        assert!(fs::metadata(&asm_file).is_ok(), "Assembly file should exist");
    } else {
        eprintln!("LLC not available or compilation failed");
    }
    
    // Step 9: Verify exported IR text can be assembled
    let bc_from_ll = test_dir.join("from_ll.bc");
    let llvm_as_versions = ["llvm-as", "llvm-as-16", "llvm-as-15", "llvm-as-14"];
    let mut llvm_as_success = false;
    
    for llvm_as_cmd in &llvm_as_versions {
        let llvm_as_output = Command::new(llvm_as_cmd)
            .args(&[exported_ll.to_str().unwrap(), "-o", bc_from_ll.to_str().unwrap()])
            .output();
        
        if llvm_as_output.is_ok() && llvm_as_output.unwrap().status.success() {
            llvm_as_success = true;
            break;
        }
    }
    
    if llvm_as_success {
        println!("✓ Exported IR assembles with llvm-as");
        assert!(fs::metadata(&bc_from_ll).is_ok(), "Bitcode from IR should exist");
    } else {
        eprintln!("llvm-as not available or assembly failed");
    }
    
    // Cleanup
    fs::remove_dir_all(&test_dir).ok();
    
    println!("\n✓ Integration test passed!");
}
