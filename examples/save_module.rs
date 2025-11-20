// Example demonstrating loading and exporting LLVM IR modules
//
// This example shows how to:
// 1. Load an LLVM IR module from a bitcode file
// 2. Export it to bitcode (.bc) and LLVM IR text (.ll) formats
// 3. Verify the exported module can be loaded back

use llvm_ir::Module;
use std::env;

fn main() {
    // Initialize logging (optional, but useful for debugging)
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input.bc> [output_prefix]", args[0]);
        eprintln!("  input.bc: Path to an LLVM bitcode file");
        eprintln!("  output_prefix: Optional prefix for output files (default: output)");
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_prefix = if args.len() > 2 {
        &args[2]
    } else {
        "output"
    };

    println!("Loading LLVM module from: {}", input_path);
    
    // Load the LLVM module
    let module = match Module::from_bc_path(input_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading module: {}", e);
            std::process::exit(1);
        }
    };

    // Print some information about the module
    println!("\nModule Information:");
    println!("  Name: {}", module.name);
    println!("  Source file: {}", module.source_file_name);
    println!("  Target triple: {:?}", module.target_triple);
    println!("  Data layout: {}", module.data_layout.layout_str);
    println!("  Functions: {}", module.functions.len());
    println!("  Function declarations: {}", module.func_declarations.len());
    println!("  Global variables: {}", module.global_vars.len());
    println!("  Global aliases: {}", module.global_aliases.len());

    // List functions
    if !module.functions.is_empty() {
        println!("\nDefined Functions:");
        for func in &module.functions {
            println!("  - {} ({} basic blocks)", func.name, func.basic_blocks.len());
        }
    }

    if !module.func_declarations.is_empty() {
        println!("\nFunction Declarations:");
        for func in &module.func_declarations {
            println!("  - {}", func.name);
        }
    }

    // Export to bitcode
    let bc_output = format!("{}.bc", output_prefix);
    println!("\nExporting to bitcode: {}", bc_output);
    match module.write_bitcode_to_file(&bc_output) {
        Ok(_) => println!("  ✓ Successfully written"),
        Err(e) => {
            eprintln!("  ✗ Error: {}", e);
            std::process::exit(1);
        }
    }

    // Export to LLVM IR text
    let ll_output = format!("{}.ll", output_prefix);
    println!("Exporting to LLVM IR text: {}", ll_output);
    match module.write_ir_to_file(&ll_output) {
        Ok(_) => println!("  ✓ Successfully written"),
        Err(e) => {
            eprintln!("  ✗ Error: {}", e);
            std::process::exit(1);
        }
    }

    // Verify the exported bitcode can be loaded
    println!("\nVerifying exported bitcode can be loaded...");
    match Module::from_bc_path(&bc_output) {
        Ok(loaded) => {
            println!("  ✓ Successfully loaded");
            println!("  Verification:");
            println!("    Source file matches: {}", 
                loaded.source_file_name == module.source_file_name);
            // Functions are now exported with bodies
            println!("    Function count: {} -> {} with bodies", 
                module.functions.len(), loaded.functions.len());
            println!("    Global vars match: {}", 
                loaded.global_vars.len() == module.global_vars.len());
        }
        Err(e) => {
            eprintln!("  ✗ Error loading exported bitcode: {}", e);
            std::process::exit(1);
        }
    }

    // Verify the exported IR text can be loaded
    println!("\nVerifying exported LLVM IR text can be loaded...");
    match Module::from_ir_path(&ll_output) {
        Ok(loaded) => {
            println!("  ✓ Successfully loaded");
            println!("  Verification:");
            println!("    Source file matches: {}", 
                loaded.source_file_name == module.source_file_name);
            println!("    Function count: {} -> {} with bodies", 
                module.functions.len(), loaded.functions.len());
        }
        Err(e) => {
            eprintln!("  ✗ Error loading exported IR text: {}", e);
            std::process::exit(1);
        }
    }

    println!("\n✓ All operations completed successfully!");
    println!("\nThe implementation exports complete function bodies including");
    println!("basic blocks, instructions, and terminators.");
}
