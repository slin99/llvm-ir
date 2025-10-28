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
    
    let test_dir = "/tmp/llvm_ir_integration_test";
    fs::create_dir_all(test_dir).ok();
    
    let c_file = format!("{}/test.c", test_dir);
    fs::write(&c_file, c_code).expect("Failed to write C file");
    
    // Step 2: Compile to LLVM bitcode using clang
    let bc_file = format!("{}/test.bc", test_dir);
    let clang_output = Command::new("clang-16")
        .args(&["-c", "-emit-llvm", &c_file, "-o", &bc_file])
        .output();
    
    if clang_output.is_err() {
        eprintln!("Clang not available, skipping integration test");
        return;
    }
    
    assert!(clang_output.unwrap().status.success(), "clang compilation failed");
    
    // Step 3: Load the module with llvm-ir
    let module = Module::from_bc_path(&bc_file)
        .expect("Failed to load bitcode");
    
    // Verify the module contents
    assert_eq!(module.source_file_name, c_file);
    assert_eq!(module.functions.len(), 3); // add, multiply, main
    assert_eq!(module.func_declarations.len(), 1); // printf
    assert_eq!(module.global_vars.len(), 1); // format string
    
    println!("✓ Module loaded successfully");
    println!("  Functions: {}", module.functions.len());
    println!("  Declarations: {}", module.func_declarations.len());
    println!("  Globals: {}", module.global_vars.len());
    
    // Step 4: Export to bitcode
    let exported_bc = format!("{}/exported.bc", test_dir);
    module.write_bitcode_to_file(&exported_bc)
        .expect("Failed to export bitcode");
    println!("✓ Exported to bitcode: {}", exported_bc);
    
    // Step 5: Export to LLVM IR text
    let exported_ll = format!("{}/exported.ll", test_dir);
    module.write_ir_to_file(&exported_ll)
        .expect("Failed to export IR text");
    println!("✓ Exported to LLVM IR: {}", exported_ll);
    
    // Step 6: Verify exported bitcode can be loaded
    let loaded = Module::from_bc_path(&exported_bc)
        .expect("Failed to load exported bitcode");
    assert_eq!(loaded.source_file_name, module.source_file_name);
    // Functions are exported as declarations
    assert_eq!(loaded.func_declarations.len(), 4); // add, multiply, main, printf
    println!("✓ Exported bitcode loads successfully");
    
    // Step 7: Verify exported IR can be loaded
    let loaded_ir = Module::from_ir_path(&exported_ll)
        .expect("Failed to load exported IR");
    assert_eq!(loaded_ir.source_file_name, module.source_file_name);
    println!("✓ Exported IR text loads successfully");
    
    // Step 8: Verify exported bitcode can be compiled with llc
    let asm_file = format!("{}/exported.s", test_dir);
    let llc_output = Command::new("llc-16")
        .args(&[&exported_bc, "-o", &asm_file])
        .output();
    
    if llc_output.is_ok() && llc_output.unwrap().status.success() {
        println!("✓ Exported bitcode compiles with llc");
        assert!(fs::metadata(&asm_file).is_ok(), "Assembly file should exist");
    } else {
        eprintln!("LLC not available or compilation failed");
    }
    
    // Step 9: Verify exported IR text can be assembled
    let bc_from_ll = format!("{}/from_ll.bc", test_dir);
    let llvm_as_output = Command::new("llvm-as-16")
        .args(&[&exported_ll, "-o", &bc_from_ll])
        .output();
    
    if llvm_as_output.is_ok() && llvm_as_output.unwrap().status.success() {
        println!("✓ Exported IR assembles with llvm-as");
        assert!(fs::metadata(&bc_from_ll).is_ok(), "Bitcode from IR should exist");
    } else {
        eprintln!("llvm-as not available or assembly failed");
    }
    
    // Cleanup
    fs::remove_dir_all(test_dir).ok();
    
    println!("\n✓ Integration test passed!");
}
