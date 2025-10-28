use llvm_ir::Module;

fn main() {
    let module = Module::from_bc_path("/tmp/demo.bc").unwrap();
    
    for gv in &module.global_vars {
        println!("Global: {}", gv.name);
        println!("  Type: {:?}", gv.ty);
        if let Some(ref init) = gv.initializer {
            println!("  Initializer: {:?}", init);
        }
    }
}
