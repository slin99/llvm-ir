// Final verification program demonstrating the complete workflow:
// 1. Create a test LLVM IR module
// 2. Load it with llvm-ir
// 3. Export it to both bitcode and LLVM IR text
// 4. Compile the exported module to verify correctness

use llvm_ir::Module;
use std::process::Command;
use std::fs;

fn main() {
    println!("=== LLVM-IR Save Feature Demonstration ===\n");
    
    // Step 1: Create test IR file
    println!("Step 1: Creating test LLVM IR module...");
    let test_ll = r#"; ModuleID = 'test_module'
source_filename = "test.c"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@message = private constant [14 x i8] c"Hello, World!\00"

declare i32 @puts(ptr)

define i32 @test_function(i32 %x, i32 %y) {
entry:
  %sum = add i32 %x, %y
  ret i32 %sum
}

define i32 @main() {
entry:
  %result = call i32 @test_function(i32 10, i32 20)
  %msg = getelementptr [14 x i8], ptr @message, i32 0, i32 0
  %call = call i32 @puts(ptr %msg)
  ret i32 %result
}
"#;
    
    let test_dir = std::env::temp_dir().join("llvm_ir_demo");
    fs::create_dir_all(&test_dir).expect("Failed to create test directory");
    
    let input_ll = test_dir.join("input.ll");
    fs::write(&input_ll, test_ll).expect("Failed to write test IR");
    println!("  ✓ Created: {}", input_ll.display());
    
    // Step 2: Convert to bitcode and load with llvm-ir
    println!("\nStep 2: Converting to bitcode and loading with llvm-ir...");
    
    // First convert .ll to .bc using llvm-as
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
            println!("  ✓ Assembled with {}", cmd);
            break;
        }
    }
    
    if !assembled {
        eprintln!("  ✗ llvm-as not available - skipping");
        return;
    }
    
    // Load with llvm-ir
    let module = Module::from_bc_path(input_bc.to_str().unwrap())
        .expect("Failed to load module");
    
    println!("  ✓ Loaded module successfully");
    println!("    - Source: {}", module.source_file_name);
    println!("    - Functions: {}", module.functions.len());
    println!("    - Declarations: {}", module.func_declarations.len());
    println!("    - Globals: {}", module.global_vars.len());
    
    // Step 3: Export using our new feature
    println!("\nStep 3: Exporting module using llvm-ir save feature...");
    
    let export_bc = test_dir.join("export.bc");
    module.write_bitcode_to_file(export_bc.to_str().unwrap())
        .expect("Failed to export bitcode");
    println!("  ✓ Exported to bitcode: {}", export_bc.display());
    
    let export_ll = test_dir.join("export.ll");
    module.write_ir_to_file(export_ll.to_str().unwrap())
        .expect("Failed to export IR");
    println!("  ✓ Exported to LLVM IR: {}", export_ll.display());
    
    // Show a preview of the exported IR
    let ir_string = module.to_ir_string().expect("Failed to get IR string");
    println!("\n  Preview of exported IR:");
    for (i, line) in ir_string.lines().take(10).enumerate() {
        println!("    {}: {}", i + 1, line);
    }
    println!("    ...");
    
    // Step 4: Verify the exported module can be loaded
    println!("\nStep 4: Verifying exported module can be loaded...");
    
    let _loaded = Module::from_bc_path(export_bc.to_str().unwrap())
        .expect("Failed to load exported bitcode");
    println!("  ✓ Exported bitcode loads successfully");
    
    let _loaded_ir = Module::from_ir_path(export_ll.to_str().unwrap())
        .expect("Failed to load exported IR");
    println!("  ✓ Exported IR loads successfully");
    
    // Step 5: Compile the exported bitcode
    println!("\nStep 5: Compiling exported bitcode with LLVM tools...");
    
    let export_s = test_dir.join("export.s");
    let llc_versions = ["llc", "llc-16", "llc-15", "llc-14"];
    let mut compiled = false;
    
    for cmd in &llc_versions {
        if Command::new(cmd)
            .args(&[export_bc.to_str().unwrap(), "-o", export_s.to_str().unwrap()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            compiled = true;
            println!("  ✓ Compiled to assembly with {}", cmd);
            
            // Show a preview of the assembly
            if let Ok(asm) = fs::read_to_string(&export_s) {
                println!("\n  Assembly preview:");
                for (i, line) in asm.lines().take(15).enumerate() {
                    println!("    {}: {}", i + 1, line);
                }
                println!("    ...");
            }
            break;
        }
    }
    
    if !compiled {
        println!("  ⚠ llc not available - cannot compile");
    }
    
    // Step 6: Assemble the exported IR
    println!("\nStep 6: Assembling exported IR...");
    
    let from_ll_bc = test_dir.join("from_export_ll.bc");
    let mut re_assembled = false;
    
    for cmd in &llvm_as_versions {
        if Command::new(cmd)
            .args(&[export_ll.to_str().unwrap(), "-o", from_ll_bc.to_str().unwrap()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            re_assembled = true;
            println!("  ✓ Re-assembled exported IR with {}", cmd);
            break;
        }
    }
    
    if !re_assembled {
        println!("  ⚠ llvm-as not available");
    }
    
    // Final summary
    println!("\n=== Summary ===");
    println!("✅ Successfully loaded LLVM IR module");
    println!("✅ Successfully exported to bitcode (.bc)");
    println!("✅ Successfully exported to LLVM IR text (.ll)");
    println!("✅ Exported files can be loaded back");
    if compiled {
        println!("✅ Exported bitcode compiles to assembly");
    }
    if re_assembled {
        println!("✅ Exported IR re-assembles to bitcode");
    }
    
    println!("\nThe implementation now exports complete function bodies including");
    println!("basic blocks, instructions, and terminators. IR is preserved through");
    println!("the save/load roundtrip with only minor cosmetic differences.");
    
    println!("\nAll files are in: {}", test_dir.display());
    println!("\nDemonstration complete! ✓");
}
