use llvm_ir::Module;

fn main() {
    let test_bc_path = "tests/basic_bc/llvm16/hello.bc";
    let module = Module::from_bc_path(test_bc_path)
        .expect("Failed to load test bitcode");
    
    println!("Original module has {} functions", module.functions.len());
    for func in &module.functions {
        println!("  Function: {} with {} basic blocks", func.name, func.basic_blocks.len());
    }
    
    let ir_string = module.to_ir_string()
        .expect("Failed to convert to IR string");
    
    println!("\nGenerated IR:\n{}", ir_string);
}
