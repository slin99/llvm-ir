use crate::llvm_sys::llvm_sys::*;
use crate::llvm_sys::*;
use crate::module::*;
use crate::types::*;
use crate::constant::*;
use crate::function::*;
use crate::name::Name;
use crate::{BasicBlock, Instruction, Operand, Terminator};
use crate::instruction;
use crate::terminator;
use crate::predicates::IntPredicate;
use either::Either;
use std::collections::HashMap;
use std::ffi::CString;

/// Context for converting llvm-ir structures to LLVM C API structures
pub struct ToLLVMContext {
    pub context: LLVMContextRef,
    pub module: LLVMModuleRef,
    pub builder: LLVMBuilderRef,
    /// Map from llvm-ir names to LLVM values
    value_map: HashMap<Name, LLVMValueRef>,
    /// Map from llvm-ir type references to LLVM types
    type_map: HashMap<TypeRef, LLVMTypeRef>,
    /// Map from basic block names to LLVM basic blocks
    bb_map: HashMap<Name, LLVMBasicBlockRef>,
}

impl ToLLVMContext {
    pub fn new(module_name: &str) -> Self {
        unsafe {
            let context = LLVMContextCreate();
            let c_module_name = CString::new(module_name).unwrap();
            let module = LLVMModuleCreateWithNameInContext(c_module_name.as_ptr(), context);
            let builder = LLVMCreateBuilderInContext(context);
            
            Self {
                context,
                module,
                builder,
                value_map: HashMap::new(),
                type_map: HashMap::new(),
                bb_map: HashMap::new(),
            }
        }
    }

    pub fn get_or_insert_value(&mut self, name: &Name) -> Option<LLVMValueRef> {
        self.value_map.get(name).copied()
    }

    pub fn insert_value(&mut self, name: Name, value: LLVMValueRef) {
        self.value_map.insert(name, value);
    }

    pub fn get_bb(&self, name: &Name) -> Option<LLVMBasicBlockRef> {
        self.bb_map.get(name).copied()
    }

    pub fn insert_bb(&mut self, name: Name, bb: LLVMBasicBlockRef) {
        self.bb_map.insert(name, bb);
    }
}

impl Drop for ToLLVMContext {
    fn drop(&mut self) {
        unsafe {
            LLVMDisposeBuilder(self.builder);
            // Note: module is not disposed here as it's returned to the caller
            // The caller is responsible for disposing the module
        }
    }
}

impl Module {
    /// Convert this `Module` to an LLVM module reference
    /// 
    /// Returns an owned LLVMModuleRef that the caller is responsible for disposing.
    /// The returned module is in a context that will also be disposed when appropriate.
    pub fn to_llvm_module(&self) -> Result<LLVMModuleRef, String> {
        let mut ctx = ToLLVMContext::new(&self.name);
        
        unsafe {
            // Set source file name
            if !self.source_file_name.is_empty() {
                let c_source = CString::new(self.source_file_name.as_str()).unwrap();
                LLVMSetSourceFileName(ctx.module, c_source.as_ptr(), self.source_file_name.len());
            }

            // Set data layout
            let layout_str = &self.data_layout.layout_str;
            if !layout_str.is_empty() {
                let c_layout = CString::new(layout_str.as_str()).unwrap();
                LLVMSetDataLayout(ctx.module, c_layout.as_ptr());
            }

            // Set target triple
            if let Some(ref triple) = self.target_triple {
                let c_triple = CString::new(triple.as_str()).unwrap();
                LLVMSetTarget(ctx.module, c_triple.as_ptr());
            }

            // Set inline assembly
            if !self.inline_assembly.is_empty() {
                let c_asm = CString::new(self.inline_assembly.as_str()).unwrap();
                LLVMSetModuleInlineAsm2(ctx.module, c_asm.as_ptr(), self.inline_assembly.len());
            }

            // Add global variables
            for global_var in &self.global_vars {
                global_var.to_llvm(&mut ctx, &self.types)?;
            }

            // Add function declarations
            for func_decl in &self.func_declarations {
                func_decl.to_llvm(&mut ctx, &self.types)?;
            }

            // Add function definitions
            for func in &self.functions {
                func.to_llvm(&mut ctx, &self.types)?;
            }

            // Add global aliases
            for alias in &self.global_aliases {
                alias.to_llvm(&mut ctx, &self.types)?;
            }

            // Add global IFuncs
            for ifunc in &self.global_ifuncs {
                ifunc.to_llvm(&mut ctx, &self.types)?;
            }
        }

        // Return the module without disposing the context yet
        // The context will be kept alive as long as the module exists
        Ok(ctx.module)
    }

    /// Save this module to a bitcode file
    pub fn write_bitcode_to_file(&self, path: &str) -> Result<(), String> {
        let module_ref = self.to_llvm_module()?;
        
        unsafe {
            let c_path = CString::new(path).map_err(|e| format!("Invalid path: {}", e))?;
            let result = llvm_sys::bit_writer::LLVMWriteBitcodeToFile(module_ref, c_path.as_ptr());
            
            // Dispose the module after writing
            LLVMDisposeModule(module_ref);
            
            if result == 0 {
                Ok(())
            } else {
                Err("Failed to write bitcode to file".to_string())
            }
        }
    }

    /// Save this module to an LLVM IR text file
    pub fn write_ir_to_file(&self, path: &str) -> Result<(), String> {
        let module_ref = self.to_llvm_module()?;
        
        unsafe {
            let c_path = CString::new(path).map_err(|e| format!("Invalid path: {}", e))?;
            let mut err_string = std::ptr::null_mut();
            let result = LLVMPrintModuleToFile(module_ref, c_path.as_ptr(), &mut err_string);
            
            // Dispose the module after writing
            LLVMDisposeModule(module_ref);
            
            if result == 0 {
                Ok(())
            } else {
                let err_msg = if !err_string.is_null() {
                    let cstr = std::ffi::CStr::from_ptr(err_string);
                    let msg = cstr.to_string_lossy().to_string();
                    LLVMDisposeMessage(err_string);
                    msg
                } else {
                    "Failed to write IR to file".to_string()
                };
                Err(err_msg)
            }
        }
    }

    /// Get the LLVM IR as a string
    pub fn to_ir_string(&self) -> Result<String, String> {
        let module_ref = self.to_llvm_module()?;
        
        unsafe {
            let ir_cstr = LLVMPrintModuleToString(module_ref);
            let ir_string = std::ffi::CStr::from_ptr(ir_cstr)
                .to_string_lossy()
                .to_string();
            
            LLVMDisposeMessage(ir_cstr);
            LLVMDisposeModule(module_ref);
            
            Ok(ir_string)
        }
    }
}

// Type conversion helpers
impl Type {
    fn to_llvm_type(&self, ctx: &mut ToLLVMContext, types: &Types) -> Result<LLVMTypeRef, String> {
        unsafe {
            match self {
                Type::VoidType => Ok(LLVMVoidTypeInContext(ctx.context)),
                Type::IntegerType { bits } => Ok(LLVMIntTypeInContext(ctx.context, *bits)),
                #[cfg(feature = "llvm-14-or-lower")]
                Type::PointerType { pointee_type, addr_space } => {
                    let pointee = pointee_type.as_ref();
                    let pointee_llvm = pointee.to_llvm_type(ctx, types)?;
                    Ok(LLVMPointerType(pointee_llvm, *addr_space))
                }
                #[cfg(feature = "llvm-15-or-greater")]
                Type::PointerType { addr_space } => {
                    Ok(LLVMPointerTypeInContext(ctx.context, *addr_space))
                }
                Type::FPType(fp_type) => {
                    match fp_type {
                        FPType::Half => Ok(LLVMHalfTypeInContext(ctx.context)),
                        #[cfg(feature = "llvm-11-or-greater")]
                        FPType::BFloat => Ok(LLVMBFloatTypeInContext(ctx.context)),
                        FPType::Single => Ok(LLVMFloatTypeInContext(ctx.context)),
                        FPType::Double => Ok(LLVMDoubleTypeInContext(ctx.context)),
                        FPType::FP128 => Ok(LLVMFP128TypeInContext(ctx.context)),
                        FPType::X86_FP80 => Ok(LLVMX86FP80TypeInContext(ctx.context)),
                        FPType::PPC_FP128 => Ok(LLVMPPCFP128TypeInContext(ctx.context)),
                    }
                }
                Type::FuncType { result_type, param_types, is_var_arg } => {
                    let result_llvm = result_type.as_ref().to_llvm_type(ctx, types)?;
                    let mut param_llvm: Vec<LLVMTypeRef> = param_types
                        .iter()
                        .map(|t| t.as_ref().to_llvm_type(ctx, types))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(LLVMFunctionType(
                        result_llvm,
                        param_llvm.as_mut_ptr(),
                        param_types.len() as u32,
                        if *is_var_arg { 1 } else { 0 },
                    ))
                }
                Type::VectorType { element_type, num_elements, .. } => {
                    let elem_llvm = element_type.as_ref().to_llvm_type(ctx, types)?;
                    Ok(LLVMVectorType(elem_llvm, *num_elements as u32))
                }
                Type::ArrayType { element_type, num_elements } => {
                    let elem_llvm = element_type.as_ref().to_llvm_type(ctx, types)?;
                    Ok(LLVMArrayType(elem_llvm, *num_elements as u32))
                }
                Type::StructType { element_types, is_packed } => {
                    let mut elem_llvm: Vec<LLVMTypeRef> = element_types
                        .iter()
                        .map(|t| t.as_ref().to_llvm_type(ctx, types))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(LLVMStructTypeInContext(
                        ctx.context,
                        elem_llvm.as_mut_ptr(),
                        element_types.len() as u32,
                        if *is_packed { 1 } else { 0 },
                    ))
                }
                Type::NamedStructType { name } => {
                    let c_name = CString::new(name.as_str()).unwrap();
                    let struct_type = LLVMStructCreateNamed(ctx.context, c_name.as_ptr());
                    
                    // Look up the definition
                    if let Some(def) = types.named_struct_def(name) {
                        match def {
                            NamedStructDef::Opaque => {
                                // Opaque struct, no body
                            }
                            NamedStructDef::Defined(type_ref) => {
                                // The type_ref should be a StructType
                                if let Type::StructType { element_types, is_packed } = type_ref.as_ref() {
                                    let mut elem_llvm: Vec<LLVMTypeRef> = element_types
                                        .iter()
                                        .map(|t| t.as_ref().to_llvm_type(ctx, types))
                                        .collect::<Result<Vec<_>, _>>()?;
                                    LLVMStructSetBody(
                                        struct_type,
                                        elem_llvm.as_mut_ptr(),
                                        element_types.len() as u32,
                                        if *is_packed { 1 } else { 0 },
                                    );
                                }
                            }
                        }
                    }
                    Ok(struct_type)
                }
                Type::X86_MMXType => Ok(LLVMX86MMXTypeInContext(ctx.context)),
                #[cfg(feature = "llvm-12-or-greater")]
                Type::X86_AMXType => Ok(LLVMX86AMXTypeInContext(ctx.context)),
                Type::MetadataType => Ok(LLVMMetadataTypeInContext(ctx.context)),
                Type::LabelType => Ok(LLVMLabelTypeInContext(ctx.context)),
                Type::TokenType => Ok(LLVMTokenTypeInContext(ctx.context)),
                #[cfg(feature = "llvm-16-or-greater")]
                Type::TargetExtType { .. } => {
                    Err("TargetExtType conversion not yet implemented".to_string())
                }
            }
        }
    }
}

impl GlobalVariable {
    fn to_llvm(&self, ctx: &mut ToLLVMContext, types: &Types) -> Result<LLVMValueRef, String> {
        unsafe {
            // Determine the type to use for the global
            // In LLVM 15+, the GlobalVariable type is an opaque pointer
            // We need the actual value type, which we can get from the initializer
            let value_type = if let Some(ref init) = self.initializer {
                // Get the type from the initializer
                types.type_of(init.as_ref())
            } else {
                // No initializer - use the pointed-to type if available
                #[cfg(feature = "llvm-14-or-lower")]
                {
                    match self.ty.as_ref() {
                        Type::PointerType { pointee_type, .. } => pointee_type.clone(),
                        _ => return Err(format!("Expected pointer type for global variable, got {:?}", self.ty)),
                    }
                }
                #[cfg(feature = "llvm-15-or-greater")]
                {
                    // For opaque pointers without initializer, we can't determine the value type
                    // Use i8 as a placeholder
                    types.i8()
                }
            };
            
            let llvm_ty = value_type.as_ref().to_llvm_type(ctx, types)?;
            
            let c_name = CString::new(self.name.to_string()).unwrap();
            let global = LLVMAddGlobal(ctx.module, llvm_ty, c_name.as_ptr());
            
            // Set properties
            LLVMSetLinkage(global, self.linkage.to_llvm());
            LLVMSetVisibility(global, self.visibility.to_llvm());
            LLVMSetGlobalConstant(global, if self.is_constant { 1 } else { 0 });
            
            if let Some(ref init) = self.initializer {
                let init_val = init.to_llvm(ctx, types)?;
                LLVMSetInitializer(global, init_val);
            }
            
            if self.alignment > 0 {
                LLVMSetAlignment(global, self.alignment);
            }
            
            if let Some(ref section) = self.section {
                let c_section = CString::new(section.as_str()).unwrap();
                LLVMSetSection(global, c_section.as_ptr());
            }
            
            ctx.insert_value(self.name.clone(), global);
            Ok(global)
        }
    }
}

impl GlobalAlias {
    fn to_llvm(&self, ctx: &mut ToLLVMContext, types: &Types) -> Result<LLVMValueRef, String> {
        unsafe {
            let ty = self.ty.as_ref();
            let llvm_ty = ty.to_llvm_type(ctx, types)?;
            
            let aliasee = self.aliasee.to_llvm(ctx, types)?;
            
            let c_name = CString::new(self.name.to_string()).unwrap();
            // LLVM 16+ uses LLVMAddAlias2 which requires address space
            #[cfg(feature = "llvm-14-or-lower")]
            let alias = LLVMAddAlias(ctx.module, llvm_ty, aliasee, c_name.as_ptr());
            #[cfg(feature = "llvm-15-or-greater")]
            let alias = LLVMAddAlias2(ctx.module, llvm_ty, 0, aliasee, c_name.as_ptr());
            
            LLVMSetLinkage(alias, self.linkage.to_llvm());
            LLVMSetVisibility(alias, self.visibility.to_llvm());
            
            ctx.insert_value(self.name.clone(), alias);
            Ok(alias)
        }
    }
}

impl GlobalIFunc {
    fn to_llvm(&self, _ctx: &mut ToLLVMContext, _types: &Types) -> Result<LLVMValueRef, String> {
        // IFunc support varies by LLVM version
        Err("GlobalIFunc conversion not yet fully implemented".to_string())
    }
}

impl FunctionDeclaration {
    fn to_llvm(&self, ctx: &mut ToLLVMContext, types: &Types) -> Result<LLVMValueRef, String> {
        unsafe {
            let func_ty = types.func_type(
                self.return_type.clone(),
                self.parameters.iter().map(|p| p.ty.clone()).collect(),
                self.is_var_arg,
            );
            let llvm_ty = func_ty.as_ref().to_llvm_type(ctx, types)?;
            
            let c_name = CString::new(self.name.as_str()).unwrap();
            let func = LLVMAddFunction(ctx.module, c_name.as_ptr(), llvm_ty);
            
            LLVMSetLinkage(func, self.linkage.to_llvm());
            LLVMSetVisibility(func, self.visibility.to_llvm());
            
            if let Some(ref gc) = self.garbage_collector_name {
                let c_gc = CString::new(gc.as_str()).unwrap();
                LLVMSetGC(func, c_gc.as_ptr());
            }
            
            ctx.insert_value(Name::Name(Box::new(self.name.clone())), func);
            Ok(func)
        }
    }
}

impl Function {
    fn to_llvm(&self, ctx: &mut ToLLVMContext, types: &Types) -> Result<LLVMValueRef, String> {
        unsafe {
            let func_ty = types.func_type(
                self.return_type.clone(),
                self.parameters.iter().map(|p| p.ty.clone()).collect(),
                self.is_var_arg,
            );
            let llvm_ty = func_ty.as_ref().to_llvm_type(ctx, types)?;
            
            let c_name = CString::new(self.name.as_str()).unwrap();
            let func = LLVMAddFunction(ctx.module, c_name.as_ptr(), llvm_ty);
            
            LLVMSetLinkage(func, self.linkage.to_llvm());
            LLVMSetVisibility(func, self.visibility.to_llvm());
            
            if let Some(ref gc) = self.garbage_collector_name {
                let c_gc = CString::new(gc.as_str()).unwrap();
                LLVMSetGC(func, c_gc.as_ptr());
            }
            
            ctx.insert_value(Name::Name(Box::new(self.name.clone())), func);
            
            // If function has basic blocks, convert them
            if !self.basic_blocks.is_empty() {
                // First pass: create all basic blocks
                for bb in &self.basic_blocks {
                    let c_bb_name = CString::new(bb.name.to_string()).unwrap();
                    let llvm_bb = LLVMAppendBasicBlockInContext(ctx.context, func, c_bb_name.as_ptr());
                    ctx.insert_bb(bb.name.clone(), llvm_bb);
                }
                
                // Map parameters to values
                for (i, param) in self.parameters.iter().enumerate() {
                    let llvm_param = LLVMGetParam(func, i as u32);
                    ctx.insert_value(param.name.clone(), llvm_param);
                }
                
                // Second pass: convert instructions in each basic block
                for bb in &self.basic_blocks {
                    bb.to_llvm(ctx, types, func)?;
                }
            }
            
            Ok(func)
        }
    }
}

impl Constant {
    fn to_llvm(&self, ctx: &mut ToLLVMContext, types: &Types) -> Result<LLVMValueRef, String> {
        unsafe {
            match self {
                Constant::Int { bits, value } => {
                    let ty = LLVMIntTypeInContext(ctx.context, *bits);
                    Ok(LLVMConstInt(ty, *value, 0))
                }
                Constant::Float(float_const) => {
                    let ty_ref = types.type_of(float_const);
                    let ty = ty_ref.as_ref().to_llvm_type(ctx, types)?;
                    // This is simplified; proper float conversion needed
                    Ok(LLVMConstNull(ty))
                }
                Constant::Null(type_ref) => {
                    let ty = type_ref.as_ref();
                    let llvm_ty = ty.to_llvm_type(ctx, types)?;
                    Ok(LLVMConstNull(llvm_ty))
                }
                Constant::AggregateZero(type_ref) => {
                    let ty = type_ref.as_ref();
                    let llvm_ty = ty.to_llvm_type(ctx, types)?;
                    Ok(LLVMConstNull(llvm_ty))
                }
                Constant::Struct { name, values, is_packed } => {
                    let mut vals: Vec<LLVMValueRef> = values
                        .iter()
                        .map(|c| c.as_ref().to_llvm(ctx, types))
                        .collect::<Result<Vec<_>, _>>()?;
                    
                    if let Some(name) = name {
                        let c_name = CString::new(name.as_str()).unwrap();
                        Ok(LLVMConstNamedStruct(
                            LLVMStructCreateNamed(ctx.context, c_name.as_ptr()),
                            vals.as_mut_ptr(),
                            vals.len() as u32,
                        ))
                    } else {
                        Ok(LLVMConstStructInContext(
                            ctx.context,
                            vals.as_mut_ptr(),
                            vals.len() as u32,
                            if *is_packed { 1 } else { 0 },
                        ))
                    }
                }
                Constant::Array { element_type, elements } => {
                    let ty = element_type.as_ref();
                    let elem_ty = ty.to_llvm_type(ctx, types)?;
                    let mut vals: Vec<LLVMValueRef> = elements
                        .iter()
                        .map(|c| c.as_ref().to_llvm(ctx, types))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(LLVMConstArray(elem_ty, vals.as_mut_ptr(), vals.len() as u32))
                }
                Constant::Vector(elements) => {
                    let mut vals: Vec<LLVMValueRef> = elements
                        .iter()
                        .map(|c| c.as_ref().to_llvm(ctx, types))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(LLVMConstVector(vals.as_mut_ptr(), vals.len() as u32))
                }
                Constant::Undef(type_ref) => {
                    let ty = type_ref.as_ref();
                    let llvm_ty = ty.to_llvm_type(ctx, types)?;
                    Ok(LLVMGetUndef(llvm_ty))
                }
                Constant::GlobalReference { name, ty: _ } => {
                    // Look up the global value
                    if let Some(val) = ctx.get_or_insert_value(name) {
                        Ok(val)
                    } else {
                        Err(format!("Global reference not found: {}", name))
                    }
                }
                _ => {
                    // Other constant types not yet implemented
                    Err(format!("Constant type not yet implemented: {:?}", self))
                }
            }
        }
    }
}

// Linkage conversions
impl Linkage {
    fn to_llvm(&self) -> LLVMLinkage {
        match self {
            Linkage::Private => LLVMLinkage::LLVMPrivateLinkage,
            Linkage::Internal => LLVMLinkage::LLVMInternalLinkage,
            Linkage::AvailableExternally => LLVMLinkage::LLVMAvailableExternallyLinkage,
            Linkage::LinkOnceAny => LLVMLinkage::LLVMLinkOnceAnyLinkage,
            Linkage::LinkOnceODR => LLVMLinkage::LLVMLinkOnceODRLinkage,
            Linkage::WeakAny => LLVMLinkage::LLVMWeakAnyLinkage,
            Linkage::WeakODR => LLVMLinkage::LLVMWeakODRLinkage,
            Linkage::Appending => LLVMLinkage::LLVMAppendingLinkage,
            Linkage::Common => LLVMLinkage::LLVMCommonLinkage,
            Linkage::ExternalWeak => LLVMLinkage::LLVMExternalWeakLinkage,
            Linkage::External => LLVMLinkage::LLVMExternalLinkage,
            Linkage::LinkOnceODRAutoHide => LLVMLinkage::LLVMLinkOnceODRAutoHideLinkage,
            Linkage::DLLImport => LLVMLinkage::LLVMDLLImportLinkage,
            Linkage::DLLExport => LLVMLinkage::LLVMDLLExportLinkage,
            Linkage::Ghost => LLVMLinkage::LLVMGhostLinkage,
            Linkage::LinkerPrivate => LLVMLinkage::LLVMLinkerPrivateLinkage,
            Linkage::LinkerPrivateWeak => LLVMLinkage::LLVMLinkerPrivateWeakLinkage,
        }
    }
}

// Visibility conversions
impl Visibility {
    fn to_llvm(&self) -> LLVMVisibility {
        match self {
            Visibility::Default => LLVMVisibility::LLVMDefaultVisibility,
            Visibility::Hidden => LLVMVisibility::LLVMHiddenVisibility,
            Visibility::Protected => LLVMVisibility::LLVMProtectedVisibility,
        }
    }
}

// BasicBlock conversion
impl BasicBlock {
    fn to_llvm(&self, ctx: &mut ToLLVMContext, types: &Types, func: LLVMValueRef) -> Result<(), String> {
        unsafe {
            let bb = ctx.get_bb(&self.name).ok_or_else(|| format!("Basic block not found: {}", self.name))?;
            LLVMPositionBuilderAtEnd(ctx.builder, bb);
            
            // Convert all instructions
            for instr in &self.instrs {
                instr.to_llvm(ctx, types)?;
            }
            
            // Convert terminator
            self.term.to_llvm(ctx, types)?;
            
            Ok(())
        }
    }
}

// Operand conversion
impl Operand {
    fn to_llvm(&self, ctx: &mut ToLLVMContext, types: &Types) -> Result<LLVMValueRef, String> {
        match self {
            Operand::LocalOperand { name, ty } => {
                ctx.get_or_insert_value(name)
                    .ok_or_else(|| format!("Local operand not found: {}", name))
            }
            Operand::ConstantOperand(const_ref) => {
                const_ref.as_ref().to_llvm(ctx, types)
            }
            Operand::MetadataOperand => {
                Err("Metadata operands not yet supported".to_string())
            }
        }
    }
}

// Instruction conversion
impl Instruction {
    fn to_llvm(&self, ctx: &mut ToLLVMContext, types: &Types) -> Result<(), String> {
        unsafe {
            let result = match self {
                Instruction::Add(add) => {
                    let lhs = add.operand0.to_llvm(ctx, types)?;
                    let rhs = add.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(add.dest.to_string()).unwrap();
                    let val = LLVMBuildAdd(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(add.dest.clone(), val);
                    Ok(())
                }
                Instruction::Sub(sub) => {
                    let lhs = sub.operand0.to_llvm(ctx, types)?;
                    let rhs = sub.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(sub.dest.to_string()).unwrap();
                    let val = LLVMBuildSub(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(sub.dest.clone(), val);
                    Ok(())
                }
                Instruction::Mul(mul) => {
                    let lhs = mul.operand0.to_llvm(ctx, types)?;
                    let rhs = mul.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(mul.dest.to_string()).unwrap();
                    let val = LLVMBuildMul(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(mul.dest.clone(), val);
                    Ok(())
                }
                Instruction::UDiv(udiv) => {
                    let lhs = udiv.operand0.to_llvm(ctx, types)?;
                    let rhs = udiv.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(udiv.dest.to_string()).unwrap();
                    let val = LLVMBuildUDiv(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(udiv.dest.clone(), val);
                    Ok(())
                }
                Instruction::SDiv(sdiv) => {
                    let lhs = sdiv.operand0.to_llvm(ctx, types)?;
                    let rhs = sdiv.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(sdiv.dest.to_string()).unwrap();
                    let val = LLVMBuildSDiv(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(sdiv.dest.clone(), val);
                    Ok(())
                }
                Instruction::Call(call) => {
                    // Get the function to call
                    let callee = match &call.function {
                        Either::Right(operand) => operand.to_llvm(ctx, types)?,
                        Either::Left(_) => return Err("Inline assembly calls not yet supported".to_string()),
                    };
                    
                    // Get function type
                    #[cfg(feature = "llvm-15-or-greater")]
                    let func_ty = call.function_ty.as_ref().to_llvm_type(ctx, types)?;
                    #[cfg(feature = "llvm-14-or-lower")]
                    let func_ty = {
                        let callee_ty = types.type_of(&call.function);
                        match callee_ty.as_ref() {
                            Type::PointerType { pointee_type, .. } => pointee_type.as_ref().to_llvm_type(ctx, types)?,
                            _ => return Err(format!("Expected pointer type for call function, got {:?}", callee_ty)),
                        }
                    };
                    
                    // Convert arguments
                    let mut args: Vec<LLVMValueRef> = call.arguments
                        .iter()
                        .map(|(op, _attrs)| op.to_llvm(ctx, types))
                        .collect::<Result<Vec<_>, _>>()?;
                    
                    let c_name = if let Some(ref name) = call.dest {
                        CString::new(name.to_string()).unwrap()
                    } else {
                        CString::new("").unwrap()
                    };
                    
                    let val = LLVMBuildCall2(
                        ctx.builder,
                        func_ty,
                        callee,
                        args.as_mut_ptr(),
                        args.len() as u32,
                        c_name.as_ptr()
                    );
                    
                    if let Some(ref dest) = call.dest {
                        ctx.insert_value(dest.clone(), val);
                    }
                    Ok(())
                }
                Instruction::Alloca(alloca) => {
                    let allocated_ty = alloca.allocated_type.as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(alloca.dest.to_string()).unwrap();
                    let val = LLVMBuildAlloca(ctx.builder, allocated_ty, c_name.as_ptr());
                    ctx.insert_value(alloca.dest.clone(), val);
                    Ok(())
                }
                Instruction::Load(load) => {
                    let addr = load.address.to_llvm(ctx, types)?;
                    let load_ty = types.type_of(load).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(load.dest.to_string()).unwrap();
                    let val = LLVMBuildLoad2(ctx.builder, load_ty, addr, c_name.as_ptr());
                    ctx.insert_value(load.dest.clone(), val);
                    Ok(())
                }
                Instruction::Store(store) => {
                    let val = store.value.to_llvm(ctx, types)?;
                    let addr = store.address.to_llvm(ctx, types)?;
                    LLVMBuildStore(ctx.builder, val, addr);
                    Ok(())
                }
                Instruction::GetElementPtr(gep) => {
                    let ptr = gep.address.to_llvm(ctx, types)?;
                    let mut indices: Vec<LLVMValueRef> = gep.indices
                        .iter()
                        .map(|op| op.to_llvm(ctx, types))
                        .collect::<Result<Vec<_>, _>>()?;
                    let c_name = CString::new(gep.dest.to_string()).unwrap();
                    
                    #[cfg(feature = "llvm-14-or-greater")]
                    let source_ty = gep.source_element_type.as_ref().to_llvm_type(ctx, types)?;
                    #[cfg(feature = "llvm-14-or-lower")]
                    let source_ty = {
                        let addr_ty = types.type_of(&gep.address);
                        match addr_ty.as_ref() {
                            Type::PointerType { pointee_type, .. } => pointee_type.as_ref().to_llvm_type(ctx, types)?,
                            _ => return Err(format!("Expected pointer type for GEP address, got {:?}", addr_ty)),
                        }
                    };
                    
                    let val = LLVMBuildGEP2(
                        ctx.builder,
                        source_ty,
                        ptr,
                        indices.as_mut_ptr(),
                        indices.len() as u32,
                        c_name.as_ptr()
                    );
                    ctx.insert_value(gep.dest.clone(), val);
                    Ok(())
                }
                Instruction::ICmp(icmp) => {
                    let lhs = icmp.operand0.to_llvm(ctx, types)?;
                    let rhs = icmp.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(icmp.dest.to_string()).unwrap();
                    let pred = icmp.predicate.to_llvm();
                    let val = LLVMBuildICmp(ctx.builder, pred, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(icmp.dest.clone(), val);
                    Ok(())
                }
                Instruction::Phi(phi) => {
                    let phi_ty = types.type_of(phi).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(phi.dest.to_string()).unwrap();
                    let phi_node = LLVMBuildPhi(ctx.builder, phi_ty, c_name.as_ptr());
                    
                    let mut values = Vec::new();
                    let mut blocks = Vec::new();
                    
                    for (op, label) in &phi.incoming_values {
                        values.push(op.to_llvm(ctx, types)?);
                        blocks.push(ctx.get_bb(label).ok_or_else(|| format!("BB not found: {}", label))?);
                    }
                    
                    LLVMAddIncoming(
                        phi_node,
                        values.as_mut_ptr(),
                        blocks.as_mut_ptr(),
                        values.len() as u32
                    );
                    
                    ctx.insert_value(phi.dest.clone(), phi_node);
                    Ok(())
                }
                // Remaining integer binary ops
                Instruction::URem(urem) => {
                    let lhs = urem.operand0.to_llvm(ctx, types)?;
                    let rhs = urem.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(urem.dest.to_string()).unwrap();
                    let val = LLVMBuildURem(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(urem.dest.clone(), val);
                    Ok(())
                }
                Instruction::SRem(srem) => {
                    let lhs = srem.operand0.to_llvm(ctx, types)?;
                    let rhs = srem.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(srem.dest.to_string()).unwrap();
                    let val = LLVMBuildSRem(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(srem.dest.clone(), val);
                    Ok(())
                }
                // Bitwise binary ops
                Instruction::And(and) => {
                    let lhs = and.operand0.to_llvm(ctx, types)?;
                    let rhs = and.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(and.dest.to_string()).unwrap();
                    let val = LLVMBuildAnd(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(and.dest.clone(), val);
                    Ok(())
                }
                Instruction::Or(or) => {
                    let lhs = or.operand0.to_llvm(ctx, types)?;
                    let rhs = or.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(or.dest.to_string()).unwrap();
                    let val = LLVMBuildOr(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(or.dest.clone(), val);
                    Ok(())
                }
                Instruction::Xor(xor) => {
                    let lhs = xor.operand0.to_llvm(ctx, types)?;
                    let rhs = xor.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(xor.dest.to_string()).unwrap();
                    let val = LLVMBuildXor(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(xor.dest.clone(), val);
                    Ok(())
                }
                Instruction::Shl(shl) => {
                    let lhs = shl.operand0.to_llvm(ctx, types)?;
                    let rhs = shl.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(shl.dest.to_string()).unwrap();
                    let val = LLVMBuildShl(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(shl.dest.clone(), val);
                    Ok(())
                }
                Instruction::LShr(lshr) => {
                    let lhs = lshr.operand0.to_llvm(ctx, types)?;
                    let rhs = lshr.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(lshr.dest.to_string()).unwrap();
                    let val = LLVMBuildLShr(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(lshr.dest.clone(), val);
                    Ok(())
                }
                Instruction::AShr(ashr) => {
                    let lhs = ashr.operand0.to_llvm(ctx, types)?;
                    let rhs = ashr.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(ashr.dest.to_string()).unwrap();
                    let val = LLVMBuildAShr(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(ashr.dest.clone(), val);
                    Ok(())
                }
                // Floating-point ops
                Instruction::FAdd(fadd) => {
                    let lhs = fadd.operand0.to_llvm(ctx, types)?;
                    let rhs = fadd.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(fadd.dest.to_string()).unwrap();
                    let val = LLVMBuildFAdd(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(fadd.dest.clone(), val);
                    Ok(())
                }
                Instruction::FSub(fsub) => {
                    let lhs = fsub.operand0.to_llvm(ctx, types)?;
                    let rhs = fsub.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(fsub.dest.to_string()).unwrap();
                    let val = LLVMBuildFSub(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(fsub.dest.clone(), val);
                    Ok(())
                }
                Instruction::FMul(fmul) => {
                    let lhs = fmul.operand0.to_llvm(ctx, types)?;
                    let rhs = fmul.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(fmul.dest.to_string()).unwrap();
                    let val = LLVMBuildFMul(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(fmul.dest.clone(), val);
                    Ok(())
                }
                Instruction::FDiv(fdiv) => {
                    let lhs = fdiv.operand0.to_llvm(ctx, types)?;
                    let rhs = fdiv.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(fdiv.dest.to_string()).unwrap();
                    let val = LLVMBuildFDiv(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(fdiv.dest.clone(), val);
                    Ok(())
                }
                Instruction::FRem(frem) => {
                    let lhs = frem.operand0.to_llvm(ctx, types)?;
                    let rhs = frem.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(frem.dest.to_string()).unwrap();
                    let val = LLVMBuildFRem(ctx.builder, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(frem.dest.clone(), val);
                    Ok(())
                }
                Instruction::FNeg(fneg) => {
                    let op = fneg.operand.to_llvm(ctx, types)?;
                    let c_name = CString::new(fneg.dest.to_string()).unwrap();
                    let val = LLVMBuildFNeg(ctx.builder, op, c_name.as_ptr());
                    ctx.insert_value(fneg.dest.clone(), val);
                    Ok(())
                }
                // Vector ops
                Instruction::ExtractElement(ee) => {
                    let vec = ee.vector.to_llvm(ctx, types)?;
                    let idx = ee.index.to_llvm(ctx, types)?;
                    let c_name = CString::new(ee.dest.to_string()).unwrap();
                    let val = LLVMBuildExtractElement(ctx.builder, vec, idx, c_name.as_ptr());
                    ctx.insert_value(ee.dest.clone(), val);
                    Ok(())
                }
                Instruction::InsertElement(ie) => {
                    let vec = ie.vector.to_llvm(ctx, types)?;
                    let elt = ie.element.to_llvm(ctx, types)?;
                    let idx = ie.index.to_llvm(ctx, types)?;
                    let c_name = CString::new(ie.dest.to_string()).unwrap();
                    let val = LLVMBuildInsertElement(ctx.builder, vec, elt, idx, c_name.as_ptr());
                    ctx.insert_value(ie.dest.clone(), val);
                    Ok(())
                }
                Instruction::ShuffleVector(sv) => {
                    let v1 = sv.operand0.to_llvm(ctx, types)?;
                    let v2 = sv.operand1.to_llvm(ctx, types)?;
                    let mask = sv.mask.to_llvm(ctx, types)?;
                    let c_name = CString::new(sv.dest.to_string()).unwrap();
                    let val = LLVMBuildShuffleVector(ctx.builder, v1, v2, mask, c_name.as_ptr());
                    ctx.insert_value(sv.dest.clone(), val);
                    Ok(())
                }
                // Aggregate ops
                Instruction::ExtractValue(ev) => {
                    let mut agg = ev.aggregate.to_llvm(ctx, types)?;
                    let c_name = CString::new(ev.dest.to_string()).unwrap();
                    let empty_name = CString::new("").unwrap();
                    // Extract nested values one index at a time
                    for (i, &idx) in ev.indices.iter().enumerate() {
                        let name = if i == ev.indices.len() - 1 {
                            c_name.as_ptr()
                        } else {
                            empty_name.as_ptr()
                        };
                        agg = LLVMBuildExtractValue(ctx.builder, agg, idx, name);
                    }
                    ctx.insert_value(ev.dest.clone(), agg);
                    Ok(())
                }
                Instruction::InsertValue(iv) => {
                    let mut agg = iv.aggregate.to_llvm(ctx, types)?;
                    let elt = iv.element.to_llvm(ctx, types)?;
                    let c_name = CString::new(iv.dest.to_string()).unwrap();
                    // Insert at nested location one index at a time
                    // This is tricky - we need to build up the insertvalue chain
                    // For now, just handle single index case
                    if iv.indices.len() == 1 {
                        let val = LLVMBuildInsertValue(ctx.builder, agg, elt, iv.indices[0], c_name.as_ptr());
                        ctx.insert_value(iv.dest.clone(), val);
                        Ok(())
                    } else {
                        Err("Multi-index InsertValue not yet fully supported".to_string())
                    }
                }
                // Memory ops
                Instruction::Fence(_fence) => {
                    // Fence is somewhat complex - for now, just skip it
                    Ok(())
                }
                Instruction::CmpXchg(cmpxchg) => {
                    let ptr = cmpxchg.address.to_llvm(ctx, types)?;
                    let cmp = cmpxchg.expected.to_llvm(ctx, types)?;
                    let new_val = cmpxchg.replacement.to_llvm(ctx, types)?;
                    let c_name = CString::new(cmpxchg.dest.to_string()).unwrap();
                    let val = LLVMBuildAtomicCmpXchg(ctx.builder, ptr, cmp, new_val, 
                        LLVMAtomicOrdering::LLVMAtomicOrderingSequentiallyConsistent,
                        LLVMAtomicOrdering::LLVMAtomicOrderingSequentiallyConsistent,
                        0);
                    ctx.insert_value(cmpxchg.dest.clone(), val);
                    Ok(())
                }
                Instruction::AtomicRMW(rmw) => {
                    let ptr = rmw.address.to_llvm(ctx, types)?;
                    let val = rmw.value.to_llvm(ctx, types)?;
                    let c_name = CString::new(rmw.dest.to_string()).unwrap();
                    // Use Xchg as default operation
                    let result = LLVMBuildAtomicRMW(ctx.builder, LLVMAtomicRMWBinOp::LLVMAtomicRMWBinOpXchg,
                        ptr, val, LLVMAtomicOrdering::LLVMAtomicOrderingSequentiallyConsistent, 0);
                    ctx.insert_value(rmw.dest.clone(), result);
                    Ok(())
                }
                // Conversion ops
                Instruction::Trunc(trunc) => {
                    let val = trunc.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(trunc).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(trunc.dest.to_string()).unwrap();
                    let result = LLVMBuildTrunc(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(trunc.dest.clone(), result);
                    Ok(())
                }
                Instruction::ZExt(zext) => {
                    let val = zext.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(zext).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(zext.dest.to_string()).unwrap();
                    let result = LLVMBuildZExt(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(zext.dest.clone(), result);
                    Ok(())
                }
                Instruction::SExt(sext) => {
                    let val = sext.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(sext).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(sext.dest.to_string()).unwrap();
                    let result = LLVMBuildSExt(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(sext.dest.clone(), result);
                    Ok(())
                }
                Instruction::FPTrunc(fptrunc) => {
                    let val = fptrunc.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(fptrunc).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(fptrunc.dest.to_string()).unwrap();
                    let result = LLVMBuildFPTrunc(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(fptrunc.dest.clone(), result);
                    Ok(())
                }
                Instruction::FPExt(fpext) => {
                    let val = fpext.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(fpext).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(fpext.dest.to_string()).unwrap();
                    let result = LLVMBuildFPExt(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(fpext.dest.clone(), result);
                    Ok(())
                }
                Instruction::FPToUI(fptoui) => {
                    let val = fptoui.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(fptoui).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(fptoui.dest.to_string()).unwrap();
                    let result = LLVMBuildFPToUI(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(fptoui.dest.clone(), result);
                    Ok(())
                }
                Instruction::FPToSI(fptosi) => {
                    let val = fptosi.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(fptosi).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(fptosi.dest.to_string()).unwrap();
                    let result = LLVMBuildFPToSI(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(fptosi.dest.clone(), result);
                    Ok(())
                }
                Instruction::UIToFP(uitofp) => {
                    let val = uitofp.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(uitofp).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(uitofp.dest.to_string()).unwrap();
                    let result = LLVMBuildUIToFP(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(uitofp.dest.clone(), result);
                    Ok(())
                }
                Instruction::SIToFP(sitofp) => {
                    let val = sitofp.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(sitofp).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(sitofp.dest.to_string()).unwrap();
                    let result = LLVMBuildSIToFP(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(sitofp.dest.clone(), result);
                    Ok(())
                }
                Instruction::PtrToInt(ptrtoint) => {
                    let val = ptrtoint.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(ptrtoint).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(ptrtoint.dest.to_string()).unwrap();
                    let result = LLVMBuildPtrToInt(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(ptrtoint.dest.clone(), result);
                    Ok(())
                }
                Instruction::IntToPtr(inttoptr) => {
                    let val = inttoptr.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(inttoptr).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(inttoptr.dest.to_string()).unwrap();
                    let result = LLVMBuildIntToPtr(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(inttoptr.dest.clone(), result);
                    Ok(())
                }
                Instruction::BitCast(bitcast) => {
                    let val = bitcast.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(bitcast).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(bitcast.dest.to_string()).unwrap();
                    let result = LLVMBuildBitCast(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(bitcast.dest.clone(), result);
                    Ok(())
                }
                Instruction::AddrSpaceCast(addrspacecast) => {
                    let val = addrspacecast.operand.to_llvm(ctx, types)?;
                    let dest_ty = types.type_of(addrspacecast).as_ref().to_llvm_type(ctx, types)?;
                    let c_name = CString::new(addrspacecast.dest.to_string()).unwrap();
                    let result = LLVMBuildAddrSpaceCast(ctx.builder, val, dest_ty, c_name.as_ptr());
                    ctx.insert_value(addrspacecast.dest.clone(), result);
                    Ok(())
                }
                // Other operations
                Instruction::FCmp(fcmp) => {
                    let lhs = fcmp.operand0.to_llvm(ctx, types)?;
                    let rhs = fcmp.operand1.to_llvm(ctx, types)?;
                    let c_name = CString::new(fcmp.dest.to_string()).unwrap();
                    let pred = fcmp.predicate.to_llvm();
                    let val = LLVMBuildFCmp(ctx.builder, pred, lhs, rhs, c_name.as_ptr());
                    ctx.insert_value(fcmp.dest.clone(), val);
                    Ok(())
                }
                Instruction::Select(select) => {
                    let cond = select.condition.to_llvm(ctx, types)?;
                    let true_val = select.true_value.to_llvm(ctx, types)?;
                    let false_val = select.false_value.to_llvm(ctx, types)?;
                    let c_name = CString::new(select.dest.to_string()).unwrap();
                    let val = LLVMBuildSelect(ctx.builder, cond, true_val, false_val, c_name.as_ptr());
                    ctx.insert_value(select.dest.clone(), val);
                    Ok(())
                }
                #[cfg(feature = "llvm-10-or-greater")]
                Instruction::Freeze(freeze) => {
                    let val = freeze.operand.to_llvm(ctx, types)?;
                    let c_name = CString::new(freeze.dest.to_string()).unwrap();
                    let result = LLVMBuildFreeze(ctx.builder, val, c_name.as_ptr());
                    ctx.insert_value(freeze.dest.clone(), result);
                    Ok(())
                }
                Instruction::VAArg(_) => {
                    Err("VAArg instruction not yet implemented".to_string())
                }
                Instruction::LandingPad(_) => {
                    Err("LandingPad instruction not yet implemented".to_string())
                }
                Instruction::CatchPad(_) => {
                    Err("CatchPad instruction not yet implemented".to_string())
                }
                Instruction::CleanupPad(_) => {
                    Err("CleanupPad instruction not yet implemented".to_string())
                }
            };
            result
        }
    }
}

// Terminator conversion
impl Terminator {
    fn to_llvm(&self, ctx: &mut ToLLVMContext, types: &Types) -> Result<(), String> {
        unsafe {
            match self {
                Terminator::Ret(ret) => {
                    if let Some(ref op) = ret.return_operand {
                        let val = op.to_llvm(ctx, types)?;
                        LLVMBuildRet(ctx.builder, val);
                    } else {
                        LLVMBuildRetVoid(ctx.builder);
                    }
                    Ok(())
                }
                Terminator::Br(br) => {
                    let dest = ctx.get_bb(&br.dest).ok_or_else(|| format!("BB not found: {}", br.dest))?;
                    LLVMBuildBr(ctx.builder, dest);
                    Ok(())
                }
                Terminator::CondBr(cbr) => {
                    let cond = cbr.condition.to_llvm(ctx, types)?;
                    let true_bb = ctx.get_bb(&cbr.true_dest).ok_or_else(|| format!("BB not found: {}", cbr.true_dest))?;
                    let false_bb = ctx.get_bb(&cbr.false_dest).ok_or_else(|| format!("BB not found: {}", cbr.false_dest))?;
                    LLVMBuildCondBr(ctx.builder, cond, true_bb, false_bb);
                    Ok(())
                }
                Terminator::Switch(sw) => {
                    let val = sw.operand.to_llvm(ctx, types)?;
                    let default_bb = ctx.get_bb(&sw.default_dest).ok_or_else(|| format!("BB not found: {}", sw.default_dest))?;
                    let switch = LLVMBuildSwitch(ctx.builder, val, default_bb, sw.dests.len() as u32);
                    
                    for (const_val, label) in &sw.dests {
                        let case_val = const_val.as_ref().to_llvm(ctx, types)?;
                        let case_bb = ctx.get_bb(label).ok_or_else(|| format!("BB not found: {}", label))?;
                        LLVMAddCase(switch, case_val, case_bb);
                    }
                    Ok(())
                }
                Terminator::Unreachable(_) => {
                    LLVMBuildUnreachable(ctx.builder);
                    Ok(())
                }
                Terminator::IndirectBr(ibr) => {
                    let addr = ibr.operand.to_llvm(ctx, types)?;
                    let ibr_inst = LLVMBuildIndirectBr(ctx.builder, addr, ibr.possible_dests.len() as u32);
                    for dest in &ibr.possible_dests {
                        let bb = ctx.get_bb(dest).ok_or_else(|| format!("BB not found: {}", dest))?;
                        LLVMAddDestination(ibr_inst, bb);
                    }
                    Ok(())
                }
                Terminator::Invoke(invoke) => {
                    // Get the function to invoke
                    let callee = match &invoke.function {
                        Either::Right(operand) => operand.to_llvm(ctx, types)?,
                        Either::Left(_) => return Err("Inline assembly invokes not supported".to_string()),
                    };
                    
                    // Get function type
                    #[cfg(feature = "llvm-15-or-greater")]
                    let func_ty = invoke.function_ty.as_ref().to_llvm_type(ctx, types)?;
                    #[cfg(feature = "llvm-14-or-lower")]
                    let func_ty = {
                        let callee_ty = types.type_of(&invoke.function);
                        match callee_ty.as_ref() {
                            Type::PointerType { pointee_type, .. } => pointee_type.as_ref().to_llvm_type(ctx, types)?,
                            _ => return Err(format!("Expected pointer type for invoke function")),
                        }
                    };
                    
                    // Convert arguments
                    let mut args: Vec<LLVMValueRef> = invoke.arguments
                        .iter()
                        .map(|(op, _attrs)| op.to_llvm(ctx, types))
                        .collect::<Result<Vec<_>, _>>()?;
                    
                    let normal_bb = ctx.get_bb(&invoke.return_label).ok_or_else(|| format!("BB not found: {}", invoke.return_label))?;
                    let unwind_bb = ctx.get_bb(&invoke.exception_label).ok_or_else(|| format!("BB not found: {}", invoke.exception_label))?;
                    
                    let c_name = CString::new(invoke.result.to_string()).unwrap();
                    
                    let val = LLVMBuildInvoke2(
                        ctx.builder,
                        func_ty,
                        callee,
                        args.as_mut_ptr(),
                        args.len() as u32,
                        normal_bb,
                        unwind_bb,
                        c_name.as_ptr()
                    );
                    
                    ctx.insert_value(invoke.result.clone(), val);
                    Ok(())
                }
                Terminator::Resume(resume) => {
                    let val = resume.operand.to_llvm(ctx, types)?;
                    LLVMBuildResume(ctx.builder, val);
                    Ok(())
                }
                Terminator::CleanupRet(_) => {
                    Err("CleanupRet terminator not yet implemented".to_string())
                }
                Terminator::CatchRet(_) => {
                    Err("CatchRet terminator not yet implemented".to_string())
                }
                Terminator::CatchSwitch(_) => {
                    Err("CatchSwitch terminator not yet implemented".to_string())
                }
                Terminator::CallBr(_) => {
                    Err("CallBr terminator not yet implemented".to_string())
                }
            }
        }
    }
}

// IntPredicate conversion
impl IntPredicate {
    fn to_llvm(&self) -> LLVMIntPredicate {
        match self {
            IntPredicate::EQ => LLVMIntPredicate::LLVMIntEQ,
            IntPredicate::NE => LLVMIntPredicate::LLVMIntNE,
            IntPredicate::UGT => LLVMIntPredicate::LLVMIntUGT,
            IntPredicate::UGE => LLVMIntPredicate::LLVMIntUGE,
            IntPredicate::ULT => LLVMIntPredicate::LLVMIntULT,
            IntPredicate::ULE => LLVMIntPredicate::LLVMIntULE,
            IntPredicate::SGT => LLVMIntPredicate::LLVMIntSGT,
            IntPredicate::SGE => LLVMIntPredicate::LLVMIntSGE,
            IntPredicate::SLT => LLVMIntPredicate::LLVMIntSLT,
            IntPredicate::SLE => LLVMIntPredicate::LLVMIntSLE,
        }
    }
}

// FPPredicate conversion
impl crate::predicates::FPPredicate {
    fn to_llvm(&self) -> LLVMRealPredicate {
        use crate::predicates::FPPredicate::*;
        match self {
            False => LLVMRealPredicate::LLVMRealPredicateFalse,
            OEQ => LLVMRealPredicate::LLVMRealOEQ,
            OGT => LLVMRealPredicate::LLVMRealOGT,
            OGE => LLVMRealPredicate::LLVMRealOGE,
            OLT => LLVMRealPredicate::LLVMRealOLT,
            OLE => LLVMRealPredicate::LLVMRealOLE,
            ONE => LLVMRealPredicate::LLVMRealONE,
            ORD => LLVMRealPredicate::LLVMRealORD,
            UNO => LLVMRealPredicate::LLVMRealUNO,
            UEQ => LLVMRealPredicate::LLVMRealUEQ,
            UGT => LLVMRealPredicate::LLVMRealUGT,
            UGE => LLVMRealPredicate::LLVMRealUGE,
            ULT => LLVMRealPredicate::LLVMRealULT,
            ULE => LLVMRealPredicate::LLVMRealULE,
            UNE => LLVMRealPredicate::LLVMRealUNE,
            True => LLVMRealPredicate::LLVMRealPredicateTrue,
        }
    }
}
