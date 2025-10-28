use llvm_ir::Module;

fn init_logging() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn test_save_and_load_bitcode() {
    init_logging();
    
    // Load a simple LLVM bitcode file
    let test_bc_path = if cfg!(feature = "llvm-16") {
        "tests/basic_bc/llvm16/hello.bc"
    } else {
        panic!("This test currently only supports llvm-16");
    };
    
    let original_module = Module::from_bc_path(test_bc_path)
        .expect("Failed to load test bitcode");
    
    // Save to a temporary file
    let temp_path = "/tmp/test_output.bc";
    original_module.write_bitcode_to_file(temp_path)
        .expect("Failed to write bitcode");
    
    // Load the saved file
    let loaded_module = Module::from_bc_path(temp_path)
        .expect("Failed to load saved bitcode");
    
    // Compare basic properties
    // Functions are now exported with full bodies
    assert_eq!(original_module.source_file_name, loaded_module.source_file_name);
    assert_eq!(original_module.functions.len(), loaded_module.functions.len());
    assert_eq!(original_module.global_vars.len(), loaded_module.global_vars.len());
    
    // Clean up
    std::fs::remove_file(temp_path).ok();
}

#[test]
fn test_save_ir_text() {
    init_logging();
    
    // Load a simple LLVM bitcode file
    let test_bc_path = if cfg!(feature = "llvm-16") {
        "tests/basic_bc/llvm16/hello.bc"
    } else {
        panic!("This test currently only supports llvm-16");
    };
    
    let module = Module::from_bc_path(test_bc_path)
        .expect("Failed to load test bitcode");
    
    // Save as LLVM IR text
    let temp_path = "/tmp/test_output.ll";
    module.write_ir_to_file(temp_path)
        .expect("Failed to write IR text");
    
    // Load the saved file
    let loaded_module = Module::from_ir_path(temp_path)
        .expect("Failed to load saved IR");
    
    // Compare basic properties
    // Functions are now exported with full bodies
    assert_eq!(module.source_file_name, loaded_module.source_file_name);
    assert_eq!(module.functions.len(), loaded_module.functions.len());
    
    // Clean up
    std::fs::remove_file(temp_path).ok();
}

#[test]
fn test_to_ir_string() {
    init_logging();
    
    // Load a simple LLVM bitcode file
    let test_bc_path = if cfg!(feature = "llvm-16") {
        "tests/basic_bc/llvm16/hello.bc"
    } else {
        panic!("This test currently only supports llvm-16");
    };
    
    let module = Module::from_bc_path(test_bc_path)
        .expect("Failed to load test bitcode");
    
    // Get IR as string
    let ir_string = module.to_ir_string()
        .expect("Failed to convert to IR string");
    
    // Should contain some expected content (function definitions now)
    assert!(ir_string.contains("define"));
    assert!(ir_string.len() > 0);
}
