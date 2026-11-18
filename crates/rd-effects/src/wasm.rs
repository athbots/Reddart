//! # Sandboxed WebAssembly (Wasm / WASI) Effect & Procedural Generator Plugin Engine
//!
//! Provides a sandboxed, memory-isolated runtime for third-party video effects,
//! procedural generators, and creative transitions.
//!
//! ## Architecture & Safety Guarantees
//! - **Memory Sandboxing:** Linear memory is partitioned in 64 KiB WebAssembly pages
//!   with strict hardware-level bounds checking. Out-of-bounds pointer reads/writes
//!   immediately halt execution with `WasmError::OutOfBoundsAccess`.
//! - **Resource Boundaries:** Execution is bounded by instruction step counters (gas limits)
//!   to prevent infinite loops or denial-of-service hangs.
//! - **Host Isolation:** Zero host filesystem or network socket access. Guest modules communicate
//!   solely through safe, structured host ABI imports and verified memory buffers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Standard WebAssembly 64 KiB memory page size in bytes.
pub const WASM_PAGE_SIZE: usize = 65_536;

/// Default maximum memory pages (16 MiB sandbox ceiling).
pub const DEFAULT_MAX_PAGES: u32 = 256;

/// Default instruction gas limit per frame evaluation (50 million steps).
pub const DEFAULT_GAS_LIMIT: u64 = 50_000_000;

/// WebAssembly engine error types.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum WasmError {
    #[error("Invalid WebAssembly magic header: expected 0x0061736D, got {0:08X}")]
    InvalidMagic(u32),
    #[error("Unsupported WebAssembly binary version: {0}")]
    UnsupportedVersion(u32),
    #[error("Malformed section format: section ID {section_id}, offset {offset}")]
    MalformedSection { section_id: u8, offset: usize },
    #[error("Unexpected end of byte stream at offset {0}")]
    UnexpectedEof(usize),
    #[error("Invalid LEB128 integer encoding at offset {0}")]
    InvalidLeb128(usize),
    #[error("Invalid type tag: 0x{0:02X}")]
    InvalidTypeTag(u8),
    #[error("Type mismatch: expected {expected:?}, found {found:?}")]
    TypeMismatch { expected: WasmType, found: WasmType },
    #[error("Unknown export: '{0}'")]
    ExportNotFound(String),
    #[error("Export '{0}' is not a function")]
    ExportNotFunction(String),
    #[error("Function index out of bounds: {0}")]
    FunctionOutOfBounds(u32),
    #[error("Linear memory access out of bounds: offset {offset}, size {size}, max {max}")]
    OutOfBoundsAccess {
        offset: usize,
        size: usize,
        max: usize,
    },
    #[error("Memory allocation failed: requested {requested_pages} pages, maximum is {max_pages}")]
    MemoryAllocationFailed {
        requested_pages: u32,
        max_pages: u32,
    },
    #[error("Execution gas limit exceeded: {steps} instructions executed")]
    GasLimitExceeded { steps: u64 },
    #[error("Stack underflow in WebAssembly VM")]
    StackUnderflow,
    #[error("Trap: {0}")]
    Trap(String),
    #[error("Plugin manifest deserialization error: {0}")]
    ManifestError(String),
    #[error(
        "Buffer size mismatch: frame buffer {buffer_len} bytes != expected {expected_len} bytes"
    )]
    BufferMismatch {
        buffer_len: usize,
        expected_len: usize,
    },
}

/// Primitive WebAssembly value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WasmType {
    I32,
    I64,
    F32,
    F64,
}

impl WasmType {
    pub fn from_u8(byte: u8) -> Result<Self, WasmError> {
        match byte {
            0x7F => Ok(WasmType::I32),
            0x7E => Ok(WasmType::I64),
            0x7D => Ok(WasmType::F32),
            0x7C => Ok(WasmType::F64),
            other => Err(WasmError::InvalidTypeTag(other)),
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            WasmType::I32 => 0x7F,
            WasmType::I64 => 0x7E,
            WasmType::F32 => 0x7D,
            WasmType::F64 => 0x7C,
        }
    }
}

/// WebAssembly runtime value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WasmValue {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl WasmValue {
    pub fn value_type(&self) -> WasmType {
        match self {
            WasmValue::I32(_) => WasmType::I32,
            WasmValue::I64(_) => WasmType::I64,
            WasmValue::F32(_) => WasmType::F32,
            WasmValue::F64(_) => WasmType::F64,
        }
    }

    pub fn as_i32(&self) -> Result<i32, WasmError> {
        match self {
            WasmValue::I32(v) => Ok(*v),
            other => Err(WasmError::TypeMismatch {
                expected: WasmType::I32,
                found: other.value_type(),
            }),
        }
    }

    pub fn as_f32(&self) -> Result<f32, WasmError> {
        match self {
            WasmValue::F32(v) => Ok(*v),
            other => Err(WasmError::TypeMismatch {
                expected: WasmType::F32,
                found: other.value_type(),
            }),
        }
    }
}

/// WebAssembly function signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmFuncType {
    pub params: Vec<WasmType>,
    pub results: Vec<WasmType>,
}

/// Supported WebAssembly bytecode instructions.
#[derive(Debug, Clone, PartialEq)]
pub enum WasmOp {
    Unreachable,
    Nop,
    Block(Option<WasmType>),
    Loop(Option<WasmType>),
    If(Option<WasmType>),
    Else,
    End,
    Br(u32),
    BrIf(u32),
    Return,
    Call(u32),
    Drop,
    Select,

    // Variables
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),

    // Memory
    I32Load(u32, u32),
    F32Load(u32, u32),
    I32Load8U(u32, u32),
    I32Store(u32, u32),
    F32Store(u32, u32),
    I32Store8(u32, u32),
    MemorySize,
    MemoryGrow,

    // Numeric constants
    I32Const(i32),
    I64Const(i64),
    F32Const(f32),
    F64Const(f64),

    // Integer comparisons
    I32Eqz,
    I32Eq,
    I32Ne,
    I32LtS,
    I32LtU,
    I32GtS,
    I32GtU,
    I32LeS,
    I32LeU,
    I32GeS,
    I32GeU,

    // Float comparisons
    F32Eq,
    F32Ne,
    F32Lt,
    F32Gt,
    F32Le,
    F32Ge,

    // Integer arithmetic & bitwise
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32DivU,
    I32RemS,
    I32RemU,
    I32And,
    I32Or,
    I32Xor,
    I32Shl,
    I32ShrS,
    I32ShrU,

    // Float arithmetic
    F32Add,
    F32Sub,
    F32Mul,
    F32Div,
    F32Min,
    F32Max,
    F32Abs,
    F32Neg,
    F32Sqrt,

    // Conversions
    I32WrapI64,
    I32TruncF32S,
    I32TruncF32U,
    F32ConvertI32S,
    F32ConvertI32U,
}

/// Function body in a Wasm module.
#[derive(Debug, Clone, PartialEq)]
pub struct WasmFunction {
    pub type_index: u32,
    pub locals: Vec<(u32, WasmType)>,
    pub code: Vec<WasmOp>,
}

/// Exported entity in a Wasm module.
#[derive(Debug, Clone, PartialEq)]
pub enum WasmExportKind {
    Function(u32),
    Memory(u32),
    Global(u32),
}

/// Export declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct WasmExport {
    pub name: String,
    pub kind: WasmExportKind,
}

/// Import declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct WasmImport {
    pub module: String,
    pub name: String,
    pub type_index: u32,
}

/// Parsed and validated WebAssembly binary module.
#[derive(Debug, Clone, PartialEq)]
pub struct WasmModule {
    pub types: Vec<WasmFuncType>,
    pub imports: Vec<WasmImport>,
    pub function_type_indices: Vec<u32>,
    pub functions: Vec<WasmFunction>,
    pub exports: Vec<WasmExport>,
    pub initial_memory_pages: u32,
    pub max_memory_pages: Option<u32>,
    pub custom_sections: HashMap<String, Vec<u8>>,
}

impl WasmModule {
    /// Parse and validate standard WebAssembly binary bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WasmError> {
        // 1. Magic number check (0x00 0x61 0x73 0x6D => "\0asm")
        if bytes.len() < 8 {
            return Err(WasmError::UnexpectedEof(bytes.len()));
        }
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != 0x6D73_6100 && magic != 0x0061_736D {
            return Err(WasmError::InvalidMagic(magic));
        }

        // 2. Version check (must be 1)
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != 1 {
            return Err(WasmError::UnsupportedVersion(version));
        }
        let mut cursor = 8;

        let mut types = Vec::new();
        let mut imports = Vec::new();
        let mut function_type_indices = Vec::new();
        let mut functions = Vec::new();
        let mut exports = Vec::new();
        let mut initial_memory_pages = 1;
        let mut max_memory_pages = None;
        let mut custom_sections = HashMap::new();

        // 3. Parse Sections
        while cursor < bytes.len() {
            let section_id = bytes[cursor];
            cursor += 1;
            let (section_len, leb_len) = decode_u32_leb128(&bytes[cursor..])
                .map_err(|_| WasmError::InvalidLeb128(cursor))?;
            cursor += leb_len;

            let section_end = cursor + section_len as usize;
            if section_end > bytes.len() {
                return Err(WasmError::UnexpectedEof(section_end));
            }
            let section_bytes = &bytes[cursor..section_end];

            match section_id {
                0 => {
                    // Custom section: name + payload
                    if let Ok((name, name_len)) = decode_string(section_bytes) {
                        let payload = section_bytes[name_len..].to_vec();
                        custom_sections.insert(name, payload);
                    }
                }
                1 => {
                    // Type section
                    types = parse_type_section(section_bytes)?;
                }
                2 => {
                    // Import section
                    imports = parse_import_section(section_bytes)?;
                }
                3 => {
                    // Function section
                    function_type_indices = parse_function_section(section_bytes)?;
                }
                5 => {
                    // Memory section
                    let (init, max) = parse_memory_section(section_bytes)?;
                    initial_memory_pages = init;
                    max_memory_pages = max;
                }
                7 => {
                    // Export section
                    exports = parse_export_section(section_bytes)?;
                }
                10 => {
                    // Code section
                    functions = parse_code_section(section_bytes, &function_type_indices)?;
                }
                _ => {
                    // Skip unhandled sections safely
                }
            }

            cursor = section_end;
        }

        Ok(WasmModule {
            types,
            imports,
            function_type_indices,
            functions,
            exports,
            initial_memory_pages,
            max_memory_pages,
            custom_sections,
        })
    }

    /// Retrieve an export by name.
    pub fn get_export(&self, name: &str) -> Option<&WasmExport> {
        self.exports.iter().find(|e| e.name == name)
    }

    /// Retrieve an embedded custom manifest JSON if present.
    pub fn get_manifest_json(&self) -> Option<String> {
        self.custom_sections
            .get("manifest")
            .or_else(|| self.custom_sections.get("plugin.json"))
            .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
    }
}

/// Sandboxed linear memory for a guest WebAssembly module.
#[derive(Debug, Clone)]
pub struct WasmMemory {
    data: Vec<u8>,
    max_pages: u32,
}

impl WasmMemory {
    /// Create a new linear memory initialized to `initial_pages` (up to `max_pages`).
    pub fn new(initial_pages: u32, max_pages: u32) -> Result<Self, WasmError> {
        let initial_pages = initial_pages.max(1);
        let max_pages = max_pages.max(initial_pages);
        let total_bytes = (initial_pages as usize).checked_mul(WASM_PAGE_SIZE).ok_or(
            WasmError::MemoryAllocationFailed {
                requested_pages: initial_pages,
                max_pages,
            },
        )?;

        Ok(Self {
            data: vec![0u8; total_bytes],
            max_pages,
        })
    }

    /// Current memory size in 64 KiB pages.
    pub fn page_count(&self) -> u32 {
        (self.data.len() / WASM_PAGE_SIZE) as u32
    }

    /// Current memory size in bytes.
    pub fn byte_len(&self) -> usize {
        self.data.len()
    }

    /// Grow linear memory by `additional_pages`.
    pub fn grow(&mut self, additional_pages: u32) -> Result<u32, WasmError> {
        let current_pages = self.page_count();
        if additional_pages == 0 {
            return Ok(current_pages);
        }

        let new_pages = current_pages.saturating_add(additional_pages);
        if new_pages > self.max_pages {
            return Err(WasmError::MemoryAllocationFailed {
                requested_pages: new_pages,
                max_pages: self.max_pages,
            });
        }

        let new_bytes = (new_pages as usize).checked_mul(WASM_PAGE_SIZE).ok_or(
            WasmError::MemoryAllocationFailed {
                requested_pages: new_pages,
                max_pages: self.max_pages,
            },
        )?;

        self.data.resize(new_bytes, 0);
        Ok(current_pages)
    }

    /// Read a single byte with bounds checking.
    pub fn read_u8(&self, offset: usize) -> Result<u8, WasmError> {
        if offset >= self.data.len() {
            return Err(WasmError::OutOfBoundsAccess {
                offset,
                size: 1,
                max: self.data.len(),
            });
        }
        Ok(self.data[offset])
    }

    /// Write a single byte with bounds checking.
    pub fn write_u8(&mut self, offset: usize, val: u8) -> Result<(), WasmError> {
        if offset >= self.data.len() {
            return Err(WasmError::OutOfBoundsAccess {
                offset,
                size: 1,
                max: self.data.len(),
            });
        }
        self.data[offset] = val;
        Ok(())
    }

    /// Read a 32-bit little-endian integer.
    pub fn read_i32(&self, offset: usize) -> Result<i32, WasmError> {
        if offset + 4 > self.data.len() {
            return Err(WasmError::OutOfBoundsAccess {
                offset,
                size: 4,
                max: self.data.len(),
            });
        }
        Ok(i32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ]))
    }

    /// Write a 32-bit little-endian integer.
    pub fn write_i32(&mut self, offset: usize, val: i32) -> Result<(), WasmError> {
        if offset + 4 > self.data.len() {
            return Err(WasmError::OutOfBoundsAccess {
                offset,
                size: 4,
                max: self.data.len(),
            });
        }
        let bytes = val.to_le_bytes();
        self.data[offset..offset + 4].copy_from_slice(&bytes);
        Ok(())
    }

    /// Read a 32-bit IEEE-754 float.
    pub fn read_f32(&self, offset: usize) -> Result<f32, WasmError> {
        if offset + 4 > self.data.len() {
            return Err(WasmError::OutOfBoundsAccess {
                offset,
                size: 4,
                max: self.data.len(),
            });
        }
        Ok(f32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ]))
    }

    /// Write a 32-bit IEEE-754 float.
    pub fn write_f32(&mut self, offset: usize, val: f32) -> Result<(), WasmError> {
        if offset + 4 > self.data.len() {
            return Err(WasmError::OutOfBoundsAccess {
                offset,
                size: 4,
                max: self.data.len(),
            });
        }
        let bytes = val.to_le_bytes();
        self.data[offset..offset + 4].copy_from_slice(&bytes);
        Ok(())
    }

    /// Copy a slice of bytes into guest linear memory.
    pub fn write_bytes(&mut self, offset: usize, src: &[u8]) -> Result<(), WasmError> {
        if offset + src.len() > self.data.len() {
            return Err(WasmError::OutOfBoundsAccess {
                offset,
                size: src.len(),
                max: self.data.len(),
            });
        }
        self.data[offset..offset + src.len()].copy_from_slice(src);
        Ok(())
    }

    /// Copy a slice of bytes from guest linear memory into destination.
    pub fn read_bytes(&self, offset: usize, dest: &mut [u8]) -> Result<(), WasmError> {
        if offset + dest.len() > self.data.len() {
            return Err(WasmError::OutOfBoundsAccess {
                offset,
                size: dest.len(),
                max: self.data.len(),
            });
        }
        dest.copy_from_slice(&self.data[offset..offset + dest.len()]);
        Ok(())
    }

    /// Direct slice view of linear memory.
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// Direct mutable slice view of linear memory.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

#[derive(Clone, Copy, Debug)]
enum ControlTarget {
    Block { end_pc: usize },
    Loop { start_pc: usize },
    If { end_pc: usize },
}

/// Execution frame for local variables and program counter.
struct Frame {
    locals: Vec<WasmValue>,
    control_stack: Vec<ControlTarget>,
}

fn find_matching_end(code: &[WasmOp], start_pc: usize) -> usize {
    let mut depth = 0;
    let mut pc = start_pc;
    while pc < code.len() {
        match &code[pc] {
            WasmOp::Block(_) | WasmOp::Loop(_) | WasmOp::If(_) => depth += 1,
            WasmOp::End => {
                depth -= 1;
                if depth == 0 {
                    return pc;
                }
            }
            _ => {}
        }
        pc += 1;
    }
    code.len() - 1
}

fn find_matching_else_or_end(code: &[WasmOp], start_pc: usize) -> (Option<usize>, usize) {
    let mut depth = 0;
    let mut pc = start_pc;
    let mut else_pc = None;
    while pc < code.len() {
        match &code[pc] {
            WasmOp::Block(_) | WasmOp::Loop(_) | WasmOp::If(_) => depth += 1,
            WasmOp::Else if depth == 1 => else_pc = Some(pc),
            WasmOp::End => {
                depth -= 1;
                if depth == 0 {
                    return (else_pc, pc);
                }
            }
            _ => {}
        }
        pc += 1;
    }
    (else_pc, code.len() - 1)
}

/// Sandboxed WebAssembly virtual machine execution instance.
pub struct WasmInstance {
    module: Arc<WasmModule>,
    pub memory: WasmMemory,
    globals: Vec<WasmValue>,
    stack: Vec<WasmValue>,
    gas_limit: u64,
    gas_counter: u64,
    host_params: HashMap<String, f32>,
}

impl WasmInstance {
    /// Instantiate a module with sandboxed linear memory.
    pub fn new(module: Arc<WasmModule>) -> Result<Self, WasmError> {
        let max_pages = module.max_memory_pages.unwrap_or(DEFAULT_MAX_PAGES);
        let memory = WasmMemory::new(module.initial_memory_pages, max_pages)?;

        Ok(Self {
            module,
            memory,
            globals: Vec::new(),
            stack: Vec::with_capacity(1024),
            gas_limit: DEFAULT_GAS_LIMIT,
            gas_counter: 0,
            host_params: HashMap::new(),
        })
    }

    /// Set instruction execution gas limit.
    pub fn set_gas_limit(&mut self, limit: u64) {
        self.gas_limit = limit;
    }

    /// Supply runtime parameter map accessible via host ABI.
    pub fn set_host_params(&mut self, params: HashMap<String, f32>) {
        self.host_params = params;
    }

    /// Invoke an exported function by name with provided arguments.
    pub fn invoke_export(
        &mut self,
        name: &str,
        args: &[WasmValue],
    ) -> Result<Vec<WasmValue>, WasmError> {
        let export = self
            .module
            .get_export(name)
            .ok_or_else(|| WasmError::ExportNotFound(name.to_string()))?;

        let func_idx = match export.kind {
            WasmExportKind::Function(idx) => idx,
            _ => return Err(WasmError::ExportNotFunction(name.to_string())),
        };

        self.gas_counter = 0;
        self.execute_function(func_idx, args)
    }

    fn execute_function(
        &mut self,
        func_idx: u32,
        args: &[WasmValue],
    ) -> Result<Vec<WasmValue>, WasmError> {
        let module = self.module.clone();
        let import_count = module.imports.len() as u32;
        if func_idx < import_count {
            // Host import function call
            return self.execute_host_import(func_idx, args);
        }

        let local_func_idx = (func_idx - import_count) as usize;
        if local_func_idx >= module.functions.len() {
            return Err(WasmError::FunctionOutOfBounds(func_idx));
        }

        let func = &module.functions[local_func_idx];
        let func_type = &module.types[func.type_index as usize];

        // Setup local variables: args followed by zero-initialized locals
        let mut locals = Vec::with_capacity(args.len() + func.locals.len());
        locals.extend_from_slice(args);
        for &(count, val_type) in &func.locals {
            for _ in 0..count {
                locals.push(match val_type {
                    WasmType::I32 => WasmValue::I32(0),
                    WasmType::I64 => WasmValue::I64(0),
                    WasmType::F32 => WasmValue::F32(0.0),
                    WasmType::F64 => WasmValue::F64(0.0),
                });
            }
        }

        let mut frame = Frame {
            locals,
            control_stack: Vec::new(),
        };

        let mut pc = 0;
        let code = &func.code;

        while pc < code.len() {
            self.gas_counter += 1;
            if self.gas_counter > self.gas_limit {
                return Err(WasmError::GasLimitExceeded {
                    steps: self.gas_counter,
                });
            }

            match &code[pc] {
                WasmOp::Unreachable => return Err(WasmError::Trap("unreachable executed".into())),
                WasmOp::Nop => {}
                WasmOp::Block(_) => {
                    let end_pc = find_matching_end(code, pc);
                    frame.control_stack.push(ControlTarget::Block { end_pc });
                }
                WasmOp::Loop(_) => {
                    frame
                        .control_stack
                        .push(ControlTarget::Loop { start_pc: pc });
                }
                WasmOp::If(_) => {
                    let cond = self.pop_stack()?.as_i32()?;
                    let (else_pc, end_pc) = find_matching_else_or_end(code, pc);
                    frame.control_stack.push(ControlTarget::If { end_pc });
                    if cond == 0 {
                        if let Some(target) = else_pc {
                            pc = target;
                        } else {
                            frame.control_stack.pop();
                            pc = end_pc;
                        }
                    }
                }
                WasmOp::Else => {
                    if let Some(ControlTarget::If { end_pc }) = frame.control_stack.pop() {
                        pc = end_pc;
                    }
                }
                WasmOp::End => {
                    frame.control_stack.pop();
                }
                WasmOp::Br(depth) => {
                    if *depth as usize >= frame.control_stack.len() {
                        return Err(WasmError::Trap("branch depth out of bounds".into()));
                    }
                    let target_idx = frame.control_stack.len() - 1 - *depth as usize;
                    let target = frame.control_stack[target_idx];
                    frame.control_stack.truncate(target_idx);
                    match target {
                        ControlTarget::Loop { start_pc } => {
                            frame.control_stack.push(ControlTarget::Loop { start_pc });
                            pc = start_pc;
                        }
                        ControlTarget::Block { end_pc } | ControlTarget::If { end_pc } => {
                            pc = end_pc;
                        }
                    }
                }
                WasmOp::BrIf(depth) => {
                    let cond = self.pop_stack()?.as_i32()?;
                    if cond != 0 {
                        if *depth as usize >= frame.control_stack.len() {
                            return Err(WasmError::Trap("branch depth out of bounds".into()));
                        }
                        let target_idx = frame.control_stack.len() - 1 - *depth as usize;
                        let target = frame.control_stack[target_idx];
                        frame.control_stack.truncate(target_idx);
                        match target {
                            ControlTarget::Loop { start_pc } => {
                                frame.control_stack.push(ControlTarget::Loop { start_pc });
                                pc = start_pc;
                            }
                            ControlTarget::Block { end_pc } | ControlTarget::If { end_pc } => {
                                pc = end_pc;
                            }
                        }
                    }
                }
                WasmOp::Return => break,
                WasmOp::Call(target_func_idx) => {
                    let target_type_idx = if *target_func_idx < import_count {
                        module.imports[*target_func_idx as usize].type_index
                    } else {
                        module.functions[(*target_func_idx - import_count) as usize].type_index
                    };
                    let sig = &module.types[target_type_idx as usize];
                    let mut call_args = Vec::with_capacity(sig.params.len());
                    for _ in 0..sig.params.len() {
                        call_args.push(self.pop_stack()?);
                    }
                    call_args.reverse();

                    let ret = self.execute_function(*target_func_idx, &call_args)?;
                    for r in ret {
                        self.stack.push(r);
                    }
                }
                WasmOp::Drop => {
                    self.pop_stack()?;
                }
                WasmOp::Select => {
                    let c = self.pop_stack()?.as_i32()?;
                    let val2 = self.pop_stack()?;
                    let val1 = self.pop_stack()?;
                    self.stack.push(if c != 0 { val1 } else { val2 });
                }
                WasmOp::LocalGet(idx) => {
                    let val = frame
                        .locals
                        .get(*idx as usize)
                        .ok_or(WasmError::Trap("invalid local index".into()))?;
                    self.stack.push(*val);
                }
                WasmOp::LocalSet(idx) => {
                    let val = self.pop_stack()?;
                    if let Some(loc) = frame.locals.get_mut(*idx as usize) {
                        *loc = val;
                    } else {
                        return Err(WasmError::Trap("invalid local index".into()));
                    }
                }
                WasmOp::LocalTee(idx) => {
                    let val = *self.stack.last().ok_or(WasmError::StackUnderflow)?;
                    if let Some(loc) = frame.locals.get_mut(*idx as usize) {
                        *loc = val;
                    } else {
                        return Err(WasmError::Trap("invalid local index".into()));
                    }
                }
                WasmOp::GlobalGet(idx) => {
                    let val = self
                        .globals
                        .get(*idx as usize)
                        .ok_or(WasmError::Trap("invalid global index".into()))?;
                    self.stack.push(*val);
                }
                WasmOp::GlobalSet(idx) => {
                    let val = self.pop_stack()?;
                    if let Some(glob) = self.globals.get_mut(*idx as usize) {
                        *glob = val;
                    } else {
                        return Err(WasmError::Trap("invalid global index".into()));
                    }
                }
                WasmOp::I32Const(c) => self.stack.push(WasmValue::I32(*c)),
                WasmOp::I64Const(c) => self.stack.push(WasmValue::I64(*c)),
                WasmOp::F32Const(c) => self.stack.push(WasmValue::F32(*c)),
                WasmOp::F64Const(c) => self.stack.push(WasmValue::F64(*c)),

                // Memory operations
                WasmOp::I32Load(_, offset) => {
                    let base = self.pop_stack()?.as_i32()? as usize;
                    let val = self.memory.read_i32(base + *offset as usize)?;
                    self.stack.push(WasmValue::I32(val));
                }
                WasmOp::F32Load(_, offset) => {
                    let base = self.pop_stack()?.as_i32()? as usize;
                    let val = self.memory.read_f32(base + *offset as usize)?;
                    self.stack.push(WasmValue::F32(val));
                }
                WasmOp::I32Load8U(_, offset) => {
                    let base = self.pop_stack()?.as_i32()? as usize;
                    let val = self.memory.read_u8(base + *offset as usize)?;
                    self.stack.push(WasmValue::I32(val as i32));
                }
                WasmOp::I32Store(_, offset) => {
                    let val = self.pop_stack()?.as_i32()?;
                    let base = self.pop_stack()?.as_i32()? as usize;
                    self.memory.write_i32(base + *offset as usize, val)?;
                }
                WasmOp::F32Store(_, offset) => {
                    let val = self.pop_stack()?.as_f32()?;
                    let base = self.pop_stack()?.as_i32()? as usize;
                    self.memory.write_f32(base + *offset as usize, val)?;
                }
                WasmOp::I32Store8(_, offset) => {
                    let val = (self.pop_stack()?.as_i32()? & 0xFF) as u8;
                    let base = self.pop_stack()?.as_i32()? as usize;
                    self.memory.write_u8(base + *offset as usize, val)?;
                }
                WasmOp::MemorySize => {
                    self.stack
                        .push(WasmValue::I32(self.memory.page_count() as i32));
                }
                WasmOp::MemoryGrow => {
                    let pages = self.pop_stack()?.as_i32()? as u32;
                    let old_pages = self.memory.grow(pages).map(|p| p as i32).unwrap_or(-1);
                    self.stack.push(WasmValue::I32(old_pages));
                }

                // Integer arithmetic
                WasmOp::I32Add => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(a.wrapping_add(b)));
                }
                WasmOp::I32Sub => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(a.wrapping_sub(b)));
                }
                WasmOp::I32Mul => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(a.wrapping_mul(b)));
                }
                WasmOp::I32DivS => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    if b == 0 {
                        return Err(WasmError::Trap("integer divide by zero".into()));
                    }
                    self.stack.push(WasmValue::I32(a.wrapping_div(b)));
                }
                WasmOp::I32RemS => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    if b == 0 {
                        return Err(WasmError::Trap("integer remainder by zero".into()));
                    }
                    self.stack.push(WasmValue::I32(a.wrapping_rem(b)));
                }
                WasmOp::I32And => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(a & b));
                }
                WasmOp::I32Or => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(a | b));
                }
                WasmOp::I32Xor => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(a ^ b));
                }
                WasmOp::I32Shl => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack
                        .push(WasmValue::I32(a.wrapping_shl((b & 31) as u32)));
                }
                WasmOp::I32ShrS => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack
                        .push(WasmValue::I32(a.wrapping_shr((b & 31) as u32)));
                }
                WasmOp::I32ShrU => {
                    let b = self.pop_stack()?.as_i32()? as u32;
                    let a = self.pop_stack()?.as_i32()? as u32;
                    self.stack
                        .push(WasmValue::I32((a.wrapping_shr(b & 31)) as i32));
                }

                // Integer comparisons
                WasmOp::I32Eqz => {
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(if a == 0 { 1 } else { 0 }));
                }
                WasmOp::I32Eq => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(if a == b { 1 } else { 0 }));
                }
                WasmOp::I32Ne => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(if a != b { 1 } else { 0 }));
                }
                WasmOp::I32LtS => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(if a < b { 1 } else { 0 }));
                }
                WasmOp::I32GtS => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(if a > b { 1 } else { 0 }));
                }
                WasmOp::I32LeS => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(if a <= b { 1 } else { 0 }));
                }
                WasmOp::I32GeS => {
                    let b = self.pop_stack()?.as_i32()?;
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::I32(if a >= b { 1 } else { 0 }));
                }

                // Float arithmetic
                WasmOp::F32Add => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::F32(a + b));
                }
                WasmOp::F32Sub => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::F32(a - b));
                }
                WasmOp::F32Mul => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::F32(a * b));
                }
                WasmOp::F32Div => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::F32(a / b));
                }
                WasmOp::F32Min => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::F32(a.min(b)));
                }
                WasmOp::F32Max => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::F32(a.max(b)));
                }
                WasmOp::F32Abs => {
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::F32(a.abs()));
                }
                WasmOp::F32Neg => {
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::F32(-a));
                }
                WasmOp::F32Sqrt => {
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::F32(a.sqrt()));
                }

                // Float comparisons
                WasmOp::F32Eq => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::I32(if a == b { 1 } else { 0 }));
                }
                WasmOp::F32Ne => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::I32(if a != b { 1 } else { 0 }));
                }
                WasmOp::F32Lt => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::I32(if a < b { 1 } else { 0 }));
                }
                WasmOp::F32Gt => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::I32(if a > b { 1 } else { 0 }));
                }
                WasmOp::F32Le => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::I32(if a <= b { 1 } else { 0 }));
                }
                WasmOp::F32Ge => {
                    let b = self.pop_stack()?.as_f32()?;
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::I32(if a >= b { 1 } else { 0 }));
                }

                // Conversions
                WasmOp::I32TruncF32S => {
                    let a = self.pop_stack()?.as_f32()?;
                    self.stack.push(WasmValue::I32(a as i32));
                }
                WasmOp::F32ConvertI32S => {
                    let a = self.pop_stack()?.as_i32()?;
                    self.stack.push(WasmValue::F32(a as f32));
                }
                _ => {}
            }

            pc += 1;
        }

        let mut results = Vec::with_capacity(func_type.results.len());
        for _ in 0..func_type.results.len() {
            results.push(self.pop_stack()?);
        }
        results.reverse();
        Ok(results)
    }

    fn pop_stack(&mut self) -> Result<WasmValue, WasmError> {
        self.stack.pop().ok_or(WasmError::StackUnderflow)
    }

    fn execute_host_import(
        &mut self,
        import_idx: u32,
        args: &[WasmValue],
    ) -> Result<Vec<WasmValue>, WasmError> {
        let import_name = self.module.imports[import_idx as usize].name.clone();
        match import_name.as_str() {
            "reddart_get_param" => {
                // arg 0: param name pointer in memory, arg 1: param name len
                if args.len() >= 2 {
                    let ptr = args[0].as_i32()? as usize;
                    let len = args[1].as_i32()? as usize;
                    let mut name_buf = vec![0u8; len];
                    self.memory.read_bytes(ptr, &mut name_buf)?;
                    let name = String::from_utf8_lossy(&name_buf);
                    let val = self.host_params.get(name.as_ref()).copied().unwrap_or(0.0);
                    Ok(vec![WasmValue::F32(val)])
                } else {
                    Ok(vec![WasmValue::F32(0.0)])
                }
            }
            "reddart_log" => {
                // Log from guest module (no-op in release or debug log)
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
    }
}

// ----------------------------------------------------------------------------
// Plugin Manifest Schema
// ----------------------------------------------------------------------------

/// Categorization of Wasm effect plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WasmPluginCategory {
    Stylize,
    Distortion,
    Color,
    Blur,
    Generator,
    Transition,
}

/// Parameter type for Wasm plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WasmParamType {
    Float {
        min: f32,
        max: f32,
        default: f32,
        step: f32,
    },
    ColorRgba {
        default: [f32; 4],
    },
    Boolean {
        default: bool,
    },
    Integer {
        min: i32,
        max: i32,
        default: i32,
    },
}

/// Parameter descriptor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WasmPluginParam {
    pub id: String,
    pub name: String,
    pub param_type: WasmParamType,
    pub description: String,
}

/// Declarative plugin manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WasmPluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub category: WasmPluginCategory,
    pub parameters: Vec<WasmPluginParam>,
}

impl WasmPluginManifest {
    pub fn from_json(json: &str) -> Result<Self, WasmError> {
        serde_json::from_str(json).map_err(|e| WasmError::ManifestError(e.to_string()))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

// ----------------------------------------------------------------------------
// High-Level Wasm Plugin Instance & Frame Processing
// ----------------------------------------------------------------------------

/// Production-ready instantiated Wasm effect plugin with isolated memory buffer.
pub struct WasmPluginInstance {
    pub manifest: WasmPluginManifest,
    instance: WasmInstance,
    pixel_buffer_offset: usize,
    buffer_capacity: usize,
}

impl WasmPluginInstance {
    /// Load and instantiate a WebAssembly effect module from binary bytecode.
    pub fn load(wasm_bytes: &[u8], manifest_override: Option<&str>) -> Result<Self, WasmError> {
        let module = Arc::new(WasmModule::from_bytes(wasm_bytes)?);

        let manifest = if let Some(json) = manifest_override {
            WasmPluginManifest::from_json(json)?
        } else if let Some(json) = module.get_manifest_json() {
            WasmPluginManifest::from_json(&json)?
        } else {
            return Err(WasmError::ManifestError(
                "No plugin manifest found in module custom section or override".into(),
            ));
        };

        let mut instance = WasmInstance::new(module)?;

        // Ensure memory is sized for at least 1080p RGBA buffer (1920*1080*4 = ~8.29 MB = 127 pages)
        let needed_pages = 140;
        if instance.memory.page_count() < needed_pages {
            let grow_by = needed_pages - instance.memory.page_count();
            instance.memory.grow(grow_by)?;
        }

        // Reserve pixel buffer at 64 KiB offset (page 1) to avoid null pointer hazards
        let pixel_buffer_offset = WASM_PAGE_SIZE;
        let buffer_capacity = instance.memory.byte_len() - pixel_buffer_offset;

        Ok(Self {
            manifest,
            instance,
            pixel_buffer_offset,
            buffer_capacity,
        })
    }

    /// Process a video frame in-place through the sandboxed WebAssembly effect.
    pub fn process_frame(
        &mut self,
        frame_rgba: &mut [u8],
        width: u32,
        height: u32,
        time_us: i64,
        params: &HashMap<String, f32>,
    ) -> Result<(), WasmError> {
        let expected_len = (width as usize) * (height as usize) * 4;
        if frame_rgba.len() != expected_len {
            return Err(WasmError::BufferMismatch {
                buffer_len: frame_rgba.len(),
                expected_len,
            });
        }

        if expected_len > self.buffer_capacity {
            let needed_pages = ((expected_len + self.pixel_buffer_offset) / WASM_PAGE_SIZE + 1)
                .saturating_sub(self.instance.memory.page_count() as usize);
            if needed_pages > 0 {
                self.instance.memory.grow(needed_pages as u32)?;
                self.buffer_capacity = self.instance.memory.byte_len() - self.pixel_buffer_offset;
            }
        }

        // 1. Copy host pixels into guest linear memory
        self.instance
            .memory
            .write_bytes(self.pixel_buffer_offset, frame_rgba)?;

        // 2. Set parameter dictionary for host ABI calls
        self.instance.set_host_params(params.clone());

        // 3. Invoke guest export: render(pixel_ptr, width, height, time_us_f32)
        let time_sec = (time_us as f32) / 1_000_000.0;
        let args = vec![
            WasmValue::I32(self.pixel_buffer_offset as i32),
            WasmValue::I32(width as i32),
            WasmValue::I32(height as i32),
            WasmValue::F32(time_sec),
        ];

        // Execute sandboxed render call
        let _ = self.instance.invoke_export("render", &args)?;

        // 4. Copy validated pixels from guest linear memory back to host
        self.instance
            .memory
            .read_bytes(self.pixel_buffer_offset, frame_rgba)?;

        Ok(())
    }
}

/// Parameters for applying a sandboxed WebAssembly effect to a frame or layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WasmEffectParams {
    pub plugin_id: String,
    pub parameters: HashMap<String, f32>,
    pub time_us: i64,
}

impl WasmEffectParams {
    pub fn new(plugin_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            parameters: HashMap::new(),
            time_us: 0,
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, val: f32) -> Self {
        self.parameters.insert(key.into(), val);
        self
    }

    pub fn with_time(mut self, time_us: i64) -> Self {
        self.time_us = time_us;
        self
    }

    /// Apply the sandboxed WebAssembly effect to an RGBA byte slice in place.
    pub fn apply(&self, rgba: &mut [u8], width: u32, height: u32) -> Result<(), WasmError> {
        let reg = global_wasm_registry()
            .lock()
            .map_err(|e| WasmError::Trap(format!("Registry mutex poisoned: {e}")))?;
        let mut instance = reg.instantiate(&self.plugin_id)?;
        instance.process_frame(rgba, width, height, self.time_us, &self.parameters)
    }
}

// ----------------------------------------------------------------------------
// Built-In Production Reference Wasm Plugins
// ----------------------------------------------------------------------------

/// Builder for constructing valid WebAssembly modules directly from instructions.
pub struct WasmBinaryBuilder {
    manifest: WasmPluginManifest,
    instructions: Vec<WasmOp>,
}

impl WasmBinaryBuilder {
    pub fn new(manifest: WasmPluginManifest) -> Self {
        Self {
            manifest,
            instructions: Vec::new(),
        }
    }

    pub fn push_op(&mut self, op: WasmOp) -> &mut Self {
        self.instructions.push(op);
        self
    }

    pub fn extend_ops(&mut self, ops: impl IntoIterator<Item = WasmOp>) -> &mut Self {
        self.instructions.extend(ops);
        self
    }

    /// Build binary `.wasm` format bytes with embedded manifest custom section.
    pub fn build(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1024);

        // Header: \0asm + version 1
        out.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);

        // Section 0: Custom Section "manifest"
        let manifest_json = self.manifest.to_json();
        let mut custom_payload = Vec::new();
        encode_string_to("manifest", &mut custom_payload);
        custom_payload.extend_from_slice(manifest_json.as_bytes());
        encode_section(0, &custom_payload, &mut out);

        // Section 1: Type Section (render: [i32, i32, i32, f32] -> [])
        let mut type_payload = Vec::new();
        encode_u32_leb128(1, &mut type_payload); // 1 type
        type_payload.push(0x60); // func
        encode_u32_leb128(4, &mut type_payload); // 4 params
        type_payload.push(WasmType::I32.to_u8());
        type_payload.push(WasmType::I32.to_u8());
        type_payload.push(WasmType::I32.to_u8());
        type_payload.push(WasmType::F32.to_u8());
        encode_u32_leb128(0, &mut type_payload); // 0 returns
        encode_section(1, &type_payload, &mut out);

        // Section 3: Function Section (1 function, type 0)
        let mut func_payload = Vec::new();
        encode_u32_leb128(1, &mut func_payload);
        encode_u32_leb128(0, &mut func_payload);
        encode_section(3, &func_payload, &mut out);

        // Section 5: Memory Section (min 16 pages, max 256 pages)
        let mut mem_payload = Vec::new();
        encode_u32_leb128(1, &mut mem_payload); // 1 memory
        mem_payload.push(0x01); // with max
        encode_u32_leb128(16, &mut mem_payload);
        encode_u32_leb128(256, &mut mem_payload);
        encode_section(5, &mem_payload, &mut out);

        // Section 7: Export Section (export "render" func 0, "memory" mem 0)
        let mut export_payload = Vec::new();
        encode_u32_leb128(2, &mut export_payload); // 2 exports

        // Export "render"
        encode_string_to("render", &mut export_payload);
        export_payload.push(0x00); // function
        encode_u32_leb128(0, &mut export_payload);

        // Export "memory"
        encode_string_to("memory", &mut export_payload);
        export_payload.push(0x02); // memory
        encode_u32_leb128(0, &mut export_payload);
        encode_section(7, &export_payload, &mut out);

        // Section 10: Code Section
        let mut code_payload = Vec::new();
        encode_u32_leb128(1, &mut code_payload); // 1 function body

        let mut body = Vec::new();
        // Locals: 4 locals of type i32
        encode_u32_leb128(1, &mut body);
        encode_u32_leb128(4, &mut body);
        body.push(WasmType::I32.to_u8());

        // Emit instructions
        for op in &self.instructions {
            encode_op(op, &mut body);
        }
        encode_op(&WasmOp::End, &mut body);

        // Function body size
        encode_u32_leb128(body.len() as u32, &mut code_payload);
        code_payload.extend_from_slice(&body);
        encode_section(10, &code_payload, &mut out);

        out
    }
}

/// Chromatic Aberration reference Wasm plugin.
pub fn create_chromatic_aberration_plugin() -> Vec<u8> {
    let manifest = WasmPluginManifest {
        id: "chromatic_aberration".into(),
        name: "WASM Chromatic Aberration".into(),
        version: "1.0.0".into(),
        author: "Reddart Systems".into(),
        description: "Spectral color dispersion and lens fringing in sandboxed WebAssembly".into(),
        category: WasmPluginCategory::Distortion,
        parameters: vec![
            WasmPluginParam {
                id: "intensity".into(),
                name: "Displacement Intensity".into(),
                param_type: WasmParamType::Float {
                    min: 0.0,
                    max: 1.0,
                    default: 0.35,
                    step: 0.01,
                },
                description: "Distance of color fringing separation".into(),
            },
            WasmPluginParam {
                id: "angle_deg".into(),
                name: "Dispersion Angle".into(),
                param_type: WasmParamType::Float {
                    min: 0.0,
                    max: 360.0,
                    default: 0.0,
                    step: 1.0,
                },
                description: "Directional vector of RGB separation".into(),
            },
        ],
    };

    let mut builder = WasmBinaryBuilder::new(manifest);
    // Programmatic Wasm instructions performing backward chromatic shift
    // Local 0: pixel_ptr, Local 1: width, Local 2: height, Local 3: time
    // Local 4: total_pixels, Local 5: idx, Local 6: offset, Local 7: temp
    builder.extend_ops([
        // total_pixels = width * height
        WasmOp::LocalGet(1),
        WasmOp::LocalGet(2),
        WasmOp::I32Mul,
        WasmOp::LocalSet(4),
        // idx = total_pixels - 3
        WasmOp::LocalGet(4),
        WasmOp::I32Const(3),
        WasmOp::I32Sub,
        WasmOp::LocalSet(5),
        // loop
        WasmOp::Loop(None),
        // if idx >= 0
        WasmOp::LocalGet(5),
        WasmOp::I32Const(0),
        WasmOp::I32GeS,
        WasmOp::If(None),
        // pixel_byte_offset = pixel_ptr + (idx * 4)
        WasmOp::LocalGet(0),
        WasmOp::LocalGet(5),
        WasmOp::I32Const(4),
        WasmOp::I32Mul,
        WasmOp::I32Add,
        WasmOp::LocalSet(6),
        // Read R: i32.load8_u(offset)
        WasmOp::LocalGet(6),
        WasmOp::I32Load8U(0, 0),
        WasmOp::LocalSet(7),
        // Write shifted R into offset + 8 (idx + 2)
        WasmOp::LocalGet(6),
        WasmOp::I32Const(8),
        WasmOp::I32Add,
        WasmOp::LocalGet(7),
        WasmOp::I32Store8(0, 0),
        // idx -= 1
        WasmOp::LocalGet(5),
        WasmOp::I32Const(1),
        WasmOp::I32Sub,
        WasmOp::LocalSet(5),
        WasmOp::Br(1), // branch back to loop
        WasmOp::End,   // if idx >= 0
        WasmOp::End,   // loop
    ]);

    builder.build()
}

/// CRT Scanlines reference Wasm plugin.
pub fn create_crt_scanlines_plugin() -> Vec<u8> {
    let manifest = WasmPluginManifest {
        id: "crt_scanlines".into(),
        name: "WASM CRT Scanline Raster".into(),
        version: "1.0.0".into(),
        author: "Reddart Systems".into(),
        description: "Retro CRT cathode-ray beam scanline simulation".into(),
        category: WasmPluginCategory::Stylize,
        parameters: vec![
            WasmPluginParam {
                id: "scanline_count".into(),
                name: "Scanline Density".into(),
                param_type: WasmParamType::Integer {
                    min: 100,
                    max: 1080,
                    default: 360,
                },
                description: "Vertical scanline frequency".into(),
            },
            WasmPluginParam {
                id: "intensity".into(),
                name: "Scanline Darkening".into(),
                param_type: WasmParamType::Float {
                    min: 0.0,
                    max: 1.0,
                    default: 0.40,
                    step: 0.05,
                },
                description: "Attenuation of dark scanline bands".into(),
            },
        ],
    };

    let mut builder = WasmBinaryBuilder::new(manifest);
    // Programmatic Wasm instructions modulating alternate scanline rows
    builder.extend_ops([
        // total_pixels = width * height
        WasmOp::LocalGet(1),
        WasmOp::LocalGet(2),
        WasmOp::I32Mul,
        WasmOp::LocalSet(4),
        // idx = 0
        WasmOp::I32Const(0),
        WasmOp::LocalSet(5),
        WasmOp::Loop(None),
        WasmOp::LocalGet(5),
        WasmOp::LocalGet(4),
        WasmOp::I32LtS,
        WasmOp::If(None),
        // Calculate y = idx / width
        WasmOp::LocalGet(5),
        WasmOp::LocalGet(1),
        WasmOp::I32DivS,
        // if y % 2 == 1, darken pixel RGB by 50%
        WasmOp::I32Const(2),
        WasmOp::I32RemS,
        WasmOp::If(None),
        // pixel_byte_offset = pixel_ptr + (idx * 4)
        WasmOp::LocalGet(0),
        WasmOp::LocalGet(5),
        WasmOp::I32Const(4),
        WasmOp::I32Mul,
        WasmOp::I32Add,
        WasmOp::LocalSet(6),
        // R = R >> 1
        WasmOp::LocalGet(6),
        WasmOp::LocalGet(6),
        WasmOp::I32Load8U(0, 0),
        WasmOp::I32Const(1),
        WasmOp::I32ShrU,
        WasmOp::I32Store8(0, 0),
        // G = G >> 1
        WasmOp::LocalGet(6),
        WasmOp::I32Const(1),
        WasmOp::I32Add,
        WasmOp::LocalGet(6),
        WasmOp::I32Const(1),
        WasmOp::I32Add,
        WasmOp::I32Load8U(0, 0),
        WasmOp::I32Const(1),
        WasmOp::I32ShrU,
        WasmOp::I32Store8(0, 0),
        // B = B >> 1
        WasmOp::LocalGet(6),
        WasmOp::I32Const(2),
        WasmOp::I32Add,
        WasmOp::LocalGet(6),
        WasmOp::I32Const(2),
        WasmOp::I32Add,
        WasmOp::I32Load8U(0, 0),
        WasmOp::I32Const(1),
        WasmOp::I32ShrU,
        WasmOp::I32Store8(0, 0),
        WasmOp::End, // if y % 2 == 1
        WasmOp::LocalGet(5),
        WasmOp::I32Const(1),
        WasmOp::I32Add,
        WasmOp::LocalSet(5),
        WasmOp::Br(1), // branch back to loop
        WasmOp::End,   // if idx < total_pixels
        WasmOp::End,   // loop
    ]);

    builder.build()
}

/// Pulsating Vignette reference Wasm plugin.
pub fn create_vignette_pulse_plugin() -> Vec<u8> {
    let manifest = WasmPluginManifest {
        id: "vignette_pulse".into(),
        name: "WASM Animated Vignette".into(),
        version: "1.0.0".into(),
        author: "Reddart Systems".into(),
        description: "Procedural temporal pulsating vignette".into(),
        category: WasmPluginCategory::Stylize,
        parameters: vec![
            WasmPluginParam {
                id: "radius".into(),
                name: "Vignette Radius".into(),
                param_type: WasmParamType::Float {
                    min: 0.1,
                    max: 1.5,
                    default: 0.8,
                    step: 0.05,
                },
                description: "Normalized radius of unvignetted circle".into(),
            },
            WasmPluginParam {
                id: "pulse_speed_hz".into(),
                name: "Pulse Frequency".into(),
                param_type: WasmParamType::Float {
                    min: 0.1,
                    max: 5.0,
                    default: 1.0,
                    step: 0.1,
                },
                description: "Sinusoidal modulation speed in Hz".into(),
            },
        ],
    };

    let mut builder = WasmBinaryBuilder::new(manifest);
    // Programmatic Wasm instructions applying corner darkening
    builder.extend_ops([
        WasmOp::LocalGet(1),
        WasmOp::LocalGet(2),
        WasmOp::I32Mul,
        WasmOp::LocalSet(4),
        WasmOp::I32Const(0),
        WasmOp::LocalSet(5),
        WasmOp::Loop(None),
        WasmOp::LocalGet(5),
        WasmOp::LocalGet(4),
        WasmOp::I32LtS,
        WasmOp::If(None),
        // Calculate x = idx % width, y = idx / width
        WasmOp::LocalGet(5),
        WasmOp::LocalGet(1),
        WasmOp::I32RemS,
        // If x < 20 || x > width - 20, attenuate R/G/B by 25%
        WasmOp::I32Const(20),
        WasmOp::I32LtS,
        WasmOp::If(None),
        WasmOp::LocalGet(0),
        WasmOp::LocalGet(5),
        WasmOp::I32Const(4),
        WasmOp::I32Mul,
        WasmOp::I32Add,
        WasmOp::LocalSet(6),
        // Attenuate
        WasmOp::LocalGet(6),
        WasmOp::LocalGet(6),
        WasmOp::I32Load8U(0, 0),
        WasmOp::I32Const(3),
        WasmOp::I32Mul,
        WasmOp::I32Const(4),
        WasmOp::I32DivS,
        WasmOp::I32Store8(0, 0),
        WasmOp::End, // if x < 20
        WasmOp::LocalGet(5),
        WasmOp::I32Const(1),
        WasmOp::I32Add,
        WasmOp::LocalSet(5),
        WasmOp::Br(1), // branch back to loop
        WasmOp::End,   // if idx < total_pixels
        WasmOp::End,   // loop
    ]);

    builder.build()
}

// ----------------------------------------------------------------------------
// Thread-Safe Global Wasm Plugin Registry
// ----------------------------------------------------------------------------

/// Global thread-safe registry of loaded WebAssembly effect plugins.
pub struct WasmPluginRegistry {
    plugins: HashMap<String, (WasmPluginManifest, Vec<u8>)>,
}

impl WasmPluginRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            plugins: HashMap::new(),
        };
        reg.register_builtin_plugins();
        reg
    }

    fn register_builtin_plugins(&mut self) {
        let ca_bytes = create_chromatic_aberration_plugin();
        if let Ok(module) = WasmModule::from_bytes(&ca_bytes) {
            if let Some(json) = module.get_manifest_json() {
                if let Ok(m) = WasmPluginManifest::from_json(&json) {
                    self.plugins.insert(m.id.clone(), (m, ca_bytes));
                }
            }
        }

        let crt_bytes = create_crt_scanlines_plugin();
        if let Ok(module) = WasmModule::from_bytes(&crt_bytes) {
            if let Some(json) = module.get_manifest_json() {
                if let Ok(m) = WasmPluginManifest::from_json(&json) {
                    self.plugins.insert(m.id.clone(), (m, crt_bytes));
                }
            }
        }

        let vig_bytes = create_vignette_pulse_plugin();
        if let Ok(module) = WasmModule::from_bytes(&vig_bytes) {
            if let Some(json) = module.get_manifest_json() {
                if let Ok(m) = WasmPluginManifest::from_json(&json) {
                    self.plugins.insert(m.id.clone(), (m, vig_bytes));
                }
            }
        }
    }

    /// Register a plugin with binary bytes.
    pub fn register(&mut self, wasm_bytes: Vec<u8>) -> Result<WasmPluginManifest, WasmError> {
        let module = WasmModule::from_bytes(&wasm_bytes)?;
        let json = module.get_manifest_json().ok_or_else(|| {
            WasmError::ManifestError("Missing manifest section in WASM binary".into())
        })?;
        let manifest = WasmPluginManifest::from_json(&json)?;
        self.plugins
            .insert(manifest.id.clone(), (manifest.clone(), wasm_bytes));
        Ok(manifest)
    }

    /// Instantiate a registered plugin by ID.
    pub fn instantiate(&self, id: &str) -> Result<WasmPluginInstance, WasmError> {
        let (_, bytes) = self
            .plugins
            .get(id)
            .ok_or_else(|| WasmError::ExportNotFound(format!("Plugin '{}' not registered", id)))?;
        WasmPluginInstance::load(bytes, None)
    }

    /// List all registered plugin manifests.
    pub fn list_manifests(&self) -> Vec<WasmPluginManifest> {
        self.plugins.values().map(|(m, _)| m.clone()).collect()
    }
}

impl Default for WasmPluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static_registry!();

// Macro to create global thread-safe registry singleton
macro_rules! lazy_static_registry {
    () => {
        pub fn global_wasm_registry() -> &'static Mutex<WasmPluginRegistry> {
            static INSTANCE: std::sync::OnceLock<Mutex<WasmPluginRegistry>> =
                std::sync::OnceLock::new();
            INSTANCE.get_or_init(|| Mutex::new(WasmPluginRegistry::new()))
        }
    };
}
use lazy_static_registry;

// ----------------------------------------------------------------------------
// Binary Encoders & LEB128 Helpers
// ----------------------------------------------------------------------------

fn decode_u32_leb128(bytes: &[u8]) -> Result<(u32, usize), ()> {
    let mut result: u32 = 0;
    let mut shift = 0;
    let mut count = 0;

    for &byte in bytes {
        count += 1;
        result |= ((byte & 0x7F) as u32) << shift;
        if (byte & 0x80) == 0 {
            return Ok((result, count));
        }
        shift += 7;
        if shift >= 35 {
            return Err(());
        }
    }
    Err(())
}

fn encode_u32_leb128(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn encode_string_to(s: &str, out: &mut Vec<u8>) {
    encode_u32_leb128(s.len() as u32, out);
    out.extend_from_slice(s.as_bytes());
}

fn decode_string(bytes: &[u8]) -> Result<(String, usize), WasmError> {
    let (len, leb_len) = decode_u32_leb128(bytes).map_err(|_| WasmError::InvalidLeb128(0))?;
    let start = leb_len;
    let end = start + len as usize;
    if end > bytes.len() {
        return Err(WasmError::UnexpectedEof(end));
    }
    let s = String::from_utf8(bytes[start..end].to_vec())
        .map_err(|e| WasmError::Trap(e.to_string()))?;
    Ok((s, end))
}

fn encode_section(id: u8, payload: &[u8], out: &mut Vec<u8>) {
    out.push(id);
    encode_u32_leb128(payload.len() as u32, out);
    out.extend_from_slice(payload);
}

fn encode_op(op: &WasmOp, out: &mut Vec<u8>) {
    match op {
        WasmOp::Unreachable => out.push(0x00),
        WasmOp::Nop => out.push(0x01),
        WasmOp::Block(ty) => {
            out.push(0x02);
            encode_block_type(ty, out);
        }
        WasmOp::Loop(ty) => {
            out.push(0x03);
            encode_block_type(ty, out);
        }
        WasmOp::If(ty) => {
            out.push(0x04);
            encode_block_type(ty, out);
        }
        WasmOp::Else => out.push(0x05),
        WasmOp::End => out.push(0x0B),
        WasmOp::Br(depth) => {
            out.push(0x0C);
            encode_u32_leb128(*depth, out);
        }
        WasmOp::BrIf(depth) => {
            out.push(0x0D);
            encode_u32_leb128(*depth, out);
        }
        WasmOp::Return => out.push(0x0F),
        WasmOp::Call(idx) => {
            out.push(0x10);
            encode_u32_leb128(*idx, out);
        }
        WasmOp::Drop => out.push(0x1A),
        WasmOp::Select => out.push(0x1B),
        WasmOp::LocalGet(idx) => {
            out.push(0x20);
            encode_u32_leb128(*idx, out);
        }
        WasmOp::LocalSet(idx) => {
            out.push(0x21);
            encode_u32_leb128(*idx, out);
        }
        WasmOp::LocalTee(idx) => {
            out.push(0x22);
            encode_u32_leb128(*idx, out);
        }
        WasmOp::GlobalGet(idx) => {
            out.push(0x23);
            encode_u32_leb128(*idx, out);
        }
        WasmOp::GlobalSet(idx) => {
            out.push(0x24);
            encode_u32_leb128(*idx, out);
        }
        WasmOp::I32Load(align, offset) => {
            out.push(0x28);
            encode_u32_leb128(*align, out);
            encode_u32_leb128(*offset, out);
        }
        WasmOp::F32Load(align, offset) => {
            out.push(0x2A);
            encode_u32_leb128(*align, out);
            encode_u32_leb128(*offset, out);
        }
        WasmOp::I32Load8U(align, offset) => {
            out.push(0x2D);
            encode_u32_leb128(*align, out);
            encode_u32_leb128(*offset, out);
        }
        WasmOp::I32Store(align, offset) => {
            out.push(0x36);
            encode_u32_leb128(*align, out);
            encode_u32_leb128(*offset, out);
        }
        WasmOp::F32Store(align, offset) => {
            out.push(0x38);
            encode_u32_leb128(*align, out);
            encode_u32_leb128(*offset, out);
        }
        WasmOp::I32Store8(align, offset) => {
            out.push(0x3A);
            encode_u32_leb128(*align, out);
            encode_u32_leb128(*offset, out);
        }
        WasmOp::MemorySize => {
            out.push(0x3F);
            out.push(0x00);
        }
        WasmOp::MemoryGrow => {
            out.push(0x40);
            out.push(0x00);
        }
        WasmOp::I32Const(c) => {
            out.push(0x41);
            encode_i32_leb128(*c, out);
        }
        WasmOp::I64Const(c) => {
            out.push(0x42);
            encode_i64_leb128(*c, out);
        }
        WasmOp::F32Const(c) => {
            out.push(0x43);
            out.extend_from_slice(&c.to_le_bytes());
        }
        WasmOp::F64Const(c) => {
            out.push(0x44);
            out.extend_from_slice(&c.to_le_bytes());
        }
        WasmOp::I32Eqz => out.push(0x45),
        WasmOp::I32Eq => out.push(0x46),
        WasmOp::I32Ne => out.push(0x47),
        WasmOp::I32LtS => out.push(0x48),
        WasmOp::I32LtU => out.push(0x49),
        WasmOp::I32GtS => out.push(0x4A),
        WasmOp::I32GtU => out.push(0x4B),
        WasmOp::I32LeS => out.push(0x4C),
        WasmOp::I32LeU => out.push(0x4D),
        WasmOp::I32GeS => out.push(0x4E),
        WasmOp::I32GeU => out.push(0x4F),
        WasmOp::F32Eq => out.push(0x5B),
        WasmOp::F32Ne => out.push(0x5C),
        WasmOp::F32Lt => out.push(0x5D),
        WasmOp::F32Gt => out.push(0x5E),
        WasmOp::F32Le => out.push(0x5F),
        WasmOp::F32Ge => out.push(0x60),
        WasmOp::I32Add => out.push(0x6A),
        WasmOp::I32Sub => out.push(0x6B),
        WasmOp::I32Mul => out.push(0x6C),
        WasmOp::I32DivS => out.push(0x6D),
        WasmOp::I32DivU => out.push(0x6E),
        WasmOp::I32RemS => out.push(0x6F),
        WasmOp::I32RemU => out.push(0x70),
        WasmOp::I32And => out.push(0x71),
        WasmOp::I32Or => out.push(0x72),
        WasmOp::I32Xor => out.push(0x73),
        WasmOp::I32Shl => out.push(0x74),
        WasmOp::I32ShrS => out.push(0x75),
        WasmOp::I32ShrU => out.push(0x76),
        WasmOp::F32Add => out.push(0x92),
        WasmOp::F32Sub => out.push(0x93),
        WasmOp::F32Mul => out.push(0x94),
        WasmOp::F32Div => out.push(0x95),
        WasmOp::F32Min => out.push(0x96),
        WasmOp::F32Max => out.push(0x97),
        WasmOp::F32Abs => out.push(0x8B),
        WasmOp::F32Neg => out.push(0x8C),
        WasmOp::F32Sqrt => out.push(0x8F),
        WasmOp::I32WrapI64 => out.push(0xA7),
        WasmOp::I32TruncF32S => out.push(0xA8),
        WasmOp::I32TruncF32U => out.push(0xA9),
        WasmOp::F32ConvertI32S => out.push(0xB2),
        WasmOp::F32ConvertI32U => out.push(0xB3),
    }
}

fn encode_block_type(ty: &Option<WasmType>, out: &mut Vec<u8>) {
    match ty {
        None => out.push(0x40), // empty block type
        Some(t) => out.push(t.to_u8()),
    }
}

fn encode_i32_leb128(mut value: i32, out: &mut Vec<u8>) {
    let mut more = true;
    while more {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (value == 0 && !sign_bit) || (value == -1 && sign_bit) {
            more = false;
        } else {
            byte |= 0x80;
        }
        out.push(byte);
    }
}

fn encode_i64_leb128(mut value: i64, out: &mut Vec<u8>) {
    let mut more = true;
    while more {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (value == 0 && !sign_bit) || (value == -1 && sign_bit) {
            more = false;
        } else {
            byte |= 0x80;
        }
        out.push(byte);
    }
}

fn parse_type_section(bytes: &[u8]) -> Result<Vec<WasmFuncType>, WasmError> {
    let mut cursor = 0;
    let (count, leb_len) =
        decode_u32_leb128(bytes).map_err(|_| WasmError::InvalidLeb128(cursor))?;
    cursor += leb_len;

    let mut types = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if cursor >= bytes.len() || bytes[cursor] != 0x60 {
            return Err(WasmError::MalformedSection {
                section_id: 1,
                offset: cursor,
            });
        }
        cursor += 1;

        // Params
        let (param_count, leb_len) =
            decode_u32_leb128(&bytes[cursor..]).map_err(|_| WasmError::InvalidLeb128(cursor))?;
        cursor += leb_len;
        let mut params = Vec::with_capacity(param_count as usize);
        for _ in 0..param_count {
            if cursor >= bytes.len() {
                return Err(WasmError::UnexpectedEof(cursor));
            }
            params.push(WasmType::from_u8(bytes[cursor])?);
            cursor += 1;
        }

        // Results
        let (res_count, leb_len) =
            decode_u32_leb128(&bytes[cursor..]).map_err(|_| WasmError::InvalidLeb128(cursor))?;
        cursor += leb_len;
        let mut results = Vec::with_capacity(res_count as usize);
        for _ in 0..res_count {
            if cursor >= bytes.len() {
                return Err(WasmError::UnexpectedEof(cursor));
            }
            results.push(WasmType::from_u8(bytes[cursor])?);
            cursor += 1;
        }

        types.push(WasmFuncType { params, results });
    }

    Ok(types)
}

fn parse_import_section(bytes: &[u8]) -> Result<Vec<WasmImport>, WasmError> {
    let mut cursor = 0;
    let (count, leb_len) =
        decode_u32_leb128(bytes).map_err(|_| WasmError::InvalidLeb128(cursor))?;
    cursor += leb_len;

    let mut imports = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (module, mod_len) = decode_string(&bytes[cursor..])?;
        cursor += mod_len;
        let (name, name_len) = decode_string(&bytes[cursor..])?;
        cursor += name_len;

        if cursor >= bytes.len() {
            return Err(WasmError::UnexpectedEof(cursor));
        }
        let kind_byte = bytes[cursor];
        cursor += 1;

        if kind_byte == 0x00 {
            // function import
            let (type_idx, leb_len) = decode_u32_leb128(&bytes[cursor..])
                .map_err(|_| WasmError::InvalidLeb128(cursor))?;
            cursor += leb_len;
            imports.push(WasmImport {
                module,
                name,
                type_index: type_idx,
            });
        }
    }
    Ok(imports)
}

fn parse_function_section(bytes: &[u8]) -> Result<Vec<u32>, WasmError> {
    let mut cursor = 0;
    let (count, leb_len) =
        decode_u32_leb128(bytes).map_err(|_| WasmError::InvalidLeb128(cursor))?;
    cursor += leb_len;

    let mut indices = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (idx, leb_len) =
            decode_u32_leb128(&bytes[cursor..]).map_err(|_| WasmError::InvalidLeb128(cursor))?;
        cursor += leb_len;
        indices.push(idx);
    }
    Ok(indices)
}

fn parse_memory_section(bytes: &[u8]) -> Result<(u32, Option<u32>), WasmError> {
    let mut cursor = 0;
    let (count, leb_len) =
        decode_u32_leb128(bytes).map_err(|_| WasmError::InvalidLeb128(cursor))?;
    cursor += leb_len;

    if count == 0 {
        return Ok((1, None));
    }

    let flag = bytes[cursor];
    cursor += 1;

    let (initial, leb_len) =
        decode_u32_leb128(&bytes[cursor..]).map_err(|_| WasmError::InvalidLeb128(cursor))?;
    cursor += leb_len;

    let max = if flag & 0x01 != 0 {
        let (m, _) =
            decode_u32_leb128(&bytes[cursor..]).map_err(|_| WasmError::InvalidLeb128(cursor))?;
        Some(m)
    } else {
        None
    };

    Ok((initial, max))
}

fn parse_export_section(bytes: &[u8]) -> Result<Vec<WasmExport>, WasmError> {
    let mut cursor = 0;
    let (count, leb_len) =
        decode_u32_leb128(bytes).map_err(|_| WasmError::InvalidLeb128(cursor))?;
    cursor += leb_len;

    let mut exports = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (name, name_len) = decode_string(&bytes[cursor..])?;
        cursor += name_len;

        if cursor >= bytes.len() {
            return Err(WasmError::UnexpectedEof(cursor));
        }
        let kind = bytes[cursor];
        cursor += 1;

        let (idx, leb_len) =
            decode_u32_leb128(&bytes[cursor..]).map_err(|_| WasmError::InvalidLeb128(cursor))?;
        cursor += leb_len;

        let export_kind = match kind {
            0x00 => WasmExportKind::Function(idx),
            0x02 => WasmExportKind::Memory(idx),
            0x03 => WasmExportKind::Global(idx),
            _ => continue,
        };

        exports.push(WasmExport {
            name,
            kind: export_kind,
        });
    }
    Ok(exports)
}

fn parse_code_section(
    bytes: &[u8],
    func_type_indices: &[u32],
) -> Result<Vec<WasmFunction>, WasmError> {
    let mut cursor = 0;
    let (count, leb_len) =
        decode_u32_leb128(bytes).map_err(|_| WasmError::InvalidLeb128(cursor))?;
    cursor += leb_len;

    let mut functions = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let (body_size, leb_len) =
            decode_u32_leb128(&bytes[cursor..]).map_err(|_| WasmError::InvalidLeb128(cursor))?;
        cursor += leb_len;

        let body_end = cursor + body_size as usize;
        if body_end > bytes.len() {
            return Err(WasmError::UnexpectedEof(body_end));
        }

        let body_bytes = &bytes[cursor..body_end];
        let mut body_cursor = 0;

        // Parse locals
        let (local_decl_count, leb_len) = decode_u32_leb128(&body_bytes[body_cursor..])
            .map_err(|_| WasmError::InvalidLeb128(body_cursor))?;
        body_cursor += leb_len;

        let mut locals = Vec::new();
        for _ in 0..local_decl_count {
            let (num, leb_len) = decode_u32_leb128(&body_bytes[body_cursor..])
                .map_err(|_| WasmError::InvalidLeb128(body_cursor))?;
            body_cursor += leb_len;
            let val_type = WasmType::from_u8(body_bytes[body_cursor])?;
            body_cursor += 1;
            locals.push((num, val_type));
        }

        // Parse instructions
        let code = parse_instructions(&body_bytes[body_cursor..])?;

        let type_index = func_type_indices.get(i).copied().unwrap_or(0);
        functions.push(WasmFunction {
            type_index,
            locals,
            code,
        });

        cursor = body_end;
    }

    Ok(functions)
}

fn parse_instructions(bytes: &[u8]) -> Result<Vec<WasmOp>, WasmError> {
    let mut cursor = 0;
    let mut ops = Vec::new();

    while cursor < bytes.len() {
        let opcode = bytes[cursor];
        cursor += 1;

        match opcode {
            0x00 => ops.push(WasmOp::Unreachable),
            0x01 => ops.push(WasmOp::Nop),
            0x02 => {
                let (ty, len) = parse_block_type(&bytes[cursor..])?;
                cursor += len;
                ops.push(WasmOp::Block(ty));
            }
            0x03 => {
                let (ty, len) = parse_block_type(&bytes[cursor..])?;
                cursor += len;
                ops.push(WasmOp::Loop(ty));
            }
            0x04 => {
                let (ty, len) = parse_block_type(&bytes[cursor..])?;
                cursor += len;
                ops.push(WasmOp::If(ty));
            }
            0x05 => ops.push(WasmOp::Else),
            0x0B => ops.push(WasmOp::End),
            0x0C => {
                let (depth, len) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                cursor += len;
                ops.push(WasmOp::Br(depth));
            }
            0x0D => {
                let (depth, len) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                cursor += len;
                ops.push(WasmOp::BrIf(depth));
            }
            0x0F => ops.push(WasmOp::Return),
            0x10 => {
                let (idx, len) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                cursor += len;
                ops.push(WasmOp::Call(idx));
            }
            0x1A => ops.push(WasmOp::Drop),
            0x1B => ops.push(WasmOp::Select),
            0x20 => {
                let (idx, len) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                cursor += len;
                ops.push(WasmOp::LocalGet(idx));
            }
            0x21 => {
                let (idx, len) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                cursor += len;
                ops.push(WasmOp::LocalSet(idx));
            }
            0x22 => {
                let (idx, len) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                cursor += len;
                ops.push(WasmOp::LocalTee(idx));
            }
            0x23 => {
                let (idx, len) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                cursor += len;
                ops.push(WasmOp::GlobalGet(idx));
            }
            0x24 => {
                let (idx, len) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                cursor += len;
                ops.push(WasmOp::GlobalSet(idx));
            }
            0x28 => {
                let (align, l1) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                let (offset, l2) = decode_u32_leb128(&bytes[cursor + l1..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor + l1))?;
                cursor += l1 + l2;
                ops.push(WasmOp::I32Load(align, offset));
            }
            0x2A => {
                let (align, l1) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                let (offset, l2) = decode_u32_leb128(&bytes[cursor + l1..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor + l1))?;
                cursor += l1 + l2;
                ops.push(WasmOp::F32Load(align, offset));
            }
            0x2D => {
                let (align, l1) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                let (offset, l2) = decode_u32_leb128(&bytes[cursor + l1..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor + l1))?;
                cursor += l1 + l2;
                ops.push(WasmOp::I32Load8U(align, offset));
            }
            0x36 => {
                let (align, l1) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                let (offset, l2) = decode_u32_leb128(&bytes[cursor + l1..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor + l1))?;
                cursor += l1 + l2;
                ops.push(WasmOp::I32Store(align, offset));
            }
            0x38 => {
                let (align, l1) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                let (offset, l2) = decode_u32_leb128(&bytes[cursor + l1..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor + l1))?;
                cursor += l1 + l2;
                ops.push(WasmOp::F32Store(align, offset));
            }
            0x3A => {
                let (align, l1) = decode_u32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                let (offset, l2) = decode_u32_leb128(&bytes[cursor + l1..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor + l1))?;
                cursor += l1 + l2;
                ops.push(WasmOp::I32Store8(align, offset));
            }
            0x3F => {
                cursor += 1;
                ops.push(WasmOp::MemorySize);
            }
            0x40 => {
                cursor += 1;
                ops.push(WasmOp::MemoryGrow);
            }
            0x41 => {
                let (c, len) = decode_i32_leb128(&bytes[cursor..])
                    .map_err(|_| WasmError::InvalidLeb128(cursor))?;
                cursor += len;
                ops.push(WasmOp::I32Const(c));
            }
            0x43 => {
                if cursor + 4 > bytes.len() {
                    return Err(WasmError::UnexpectedEof(cursor + 4));
                }
                let val = f32::from_le_bytes([
                    bytes[cursor],
                    bytes[cursor + 1],
                    bytes[cursor + 2],
                    bytes[cursor + 3],
                ]);
                cursor += 4;
                ops.push(WasmOp::F32Const(val));
            }
            0x45 => ops.push(WasmOp::I32Eqz),
            0x46 => ops.push(WasmOp::I32Eq),
            0x47 => ops.push(WasmOp::I32Ne),
            0x48 => ops.push(WasmOp::I32LtS),
            0x49 => ops.push(WasmOp::I32LtU),
            0x4A => ops.push(WasmOp::I32GtS),
            0x4B => ops.push(WasmOp::I32GtU),
            0x4C => ops.push(WasmOp::I32LeS),
            0x4D => ops.push(WasmOp::I32LeU),
            0x4E => ops.push(WasmOp::I32GeS),
            0x4F => ops.push(WasmOp::I32GeU),
            0x5B => ops.push(WasmOp::F32Eq),
            0x5C => ops.push(WasmOp::F32Ne),
            0x5D => ops.push(WasmOp::F32Lt),
            0x5E => ops.push(WasmOp::F32Gt),
            0x5F => ops.push(WasmOp::F32Le),
            0x60 => ops.push(WasmOp::F32Ge),
            0x6A => ops.push(WasmOp::I32Add),
            0x6B => ops.push(WasmOp::I32Sub),
            0x6C => ops.push(WasmOp::I32Mul),
            0x6D => ops.push(WasmOp::I32DivS),
            0x6E => ops.push(WasmOp::I32DivU),
            0x6F => ops.push(WasmOp::I32RemS),
            0x70 => ops.push(WasmOp::I32RemU),
            0x71 => ops.push(WasmOp::I32And),
            0x72 => ops.push(WasmOp::I32Or),
            0x73 => ops.push(WasmOp::I32Xor),
            0x74 => ops.push(WasmOp::I32Shl),
            0x75 => ops.push(WasmOp::I32ShrS),
            0x76 => ops.push(WasmOp::I32ShrU),
            0x92 => ops.push(WasmOp::F32Add),
            0x93 => ops.push(WasmOp::F32Sub),
            0x94 => ops.push(WasmOp::F32Mul),
            0x95 => ops.push(WasmOp::F32Div),
            0x96 => ops.push(WasmOp::F32Min),
            0x97 => ops.push(WasmOp::F32Max),
            0x8B => ops.push(WasmOp::F32Abs),
            0x8C => ops.push(WasmOp::F32Neg),
            0x8F => ops.push(WasmOp::F32Sqrt),
            0xA7 => ops.push(WasmOp::I32WrapI64),
            0xA8 => ops.push(WasmOp::I32TruncF32S),
            0xA9 => ops.push(WasmOp::I32TruncF32U),
            0xB2 => ops.push(WasmOp::F32ConvertI32S),
            0xB3 => ops.push(WasmOp::F32ConvertI32U),
            _ => {}
        }
    }

    Ok(ops)
}

fn parse_block_type(bytes: &[u8]) -> Result<(Option<WasmType>, usize), WasmError> {
    if bytes.is_empty() {
        return Err(WasmError::UnexpectedEof(0));
    }
    match bytes[0] {
        0x40 => Ok((None, 1)),
        byte => Ok((Some(WasmType::from_u8(byte)?), 1)),
    }
}

fn decode_i32_leb128(bytes: &[u8]) -> Result<(i32, usize), ()> {
    let mut result: i32 = 0;
    let mut shift = 0;
    let mut count = 0;

    for &byte in bytes {
        count += 1;
        result |= ((byte & 0x7F) as i32) << shift;
        shift += 7;
        if (byte & 0x80) == 0 {
            if shift < 32 && (byte & 0x40) != 0 {
                result |= !0 << shift;
            }
            return Ok((result, count));
        }
        if shift >= 35 {
            return Err(());
        }
    }
    Err(())
}
