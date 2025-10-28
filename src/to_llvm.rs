use crate::llvm_sys::llvm_sys::*;
use crate::llvm_sys::*;
use crate::module::*;
use crate::types::*;
use crate::constant::*;
use crate::function::*;
use crate::name::Name;
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
            let ty = self.ty.as_ref();
            let llvm_ty = ty.to_llvm_type(ctx, types)?;
            
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
        // For now, just create a function declaration
        // Full function body conversion would require implementing instruction conversion
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
            
            // TODO: Implement basic block and instruction conversion
            // For now, this only creates the function signature
            
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
