//! Minimal hand-written bindings to ODPI-C (https://github.com/oracle/odpi),
//! the Oracle Database programmatic interface that loads the Oracle client
//! library at runtime. Only the subset needed by chcsv is bound.
#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_uint};

pub fn dpi_version() -> (u32, u32) {
    (
        env!("DPI_MAJOR_VERSION").parse().unwrap_or(0),
        env!("DPI_MINOR_VERSION").parse().unwrap_or(0),
    )
}

pub const NATIVE_TYPE_BYTES: u32 = 3004;
pub const ORACLE_TYPE_VARCHAR: u32 = 2001;
pub const MODE_EXEC_DEFAULT: u32 = 0;

// Old Oracle type codes (SQLT_*), as reported in QueryInfo.ociTypeCode.
pub const SQLT_CHR: u16 = 1;
pub const SQLT_NUM: u16 = 2;
pub const SQLT_LNG: u16 = 8;
pub const SQLT_DAT: u16 = 12;
pub const SQLT_RID: u16 = 13;
pub const SQLT_RDD: u16 = 11;
pub const SQLT_LNGRAW: u16 = 24;
pub const SQLT_CLOB: u16 = 112;
pub const SQLT_BLOB: u16 = 113;
pub const SQLT_NCLOB: u16 = 129;

#[repr(C)]
pub struct ErrorInfo {
    pub code: i32,
    pub offset16: u16,
    pub message: *const c_char,
    pub message_length: u32,
    pub encoding: *const c_char,
    pub fn_name: *const c_char,
    pub action: *const c_char,
    pub sql_state: *const c_char,
    pub is_recoverable: c_int,
    pub is_warning: c_int,
    pub offset: u32,
}

impl Default for ErrorInfo {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bytes {
    pub ptr: *const c_char,
    pub length: u32,
    pub encoding: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Timestamp {
    pub year: i16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub fsecond: u32,
    pub tz_hour_offset: i8,
    pub tz_minute_offset: i8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct JsonObject {
    pub num_fields: u32,
    pub field_names: *mut *mut c_char,
    pub field_name_lengths: *mut u32,
    pub fields: *mut c_void,
    pub field_values: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct JsonArray {
    pub num_elements: u32,
    pub elements: *mut c_void,
    pub element_values: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union DataBuffer {
    pub as_i64: i64,
    pub as_u64: u64,
    pub as_double: f64,
    pub as_ptr: *mut c_void,
    pub as_bytes: Bytes,
    pub as_timestamp: Timestamp,
    pub as_json_object: JsonObject,
    pub as_json_array: JsonArray,
}

#[repr(C)]
pub struct Data {
    pub is_null: c_int,
    pub value: DataBuffer,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DataTypeInfo {
    pub oracle_type_num: u32,
    pub default_native_type_num: u32,
    pub oci_type_code: u16,
    pub db_size_in_bytes: u32,
    pub client_size_in_bytes: u32,
    pub size_in_chars: u32,
    pub precision: i16,
    pub scale: i8,
    pub fs_precision: u8,
    pub object_type: *mut c_void,
    pub is_json: c_int,
    pub domain_schema: *const c_char,
    pub domain_schema_length: u32,
    pub domain_name: *const c_char,
    pub domain_name_length: u32,
    pub num_annotations: u32,
    pub annotations: *mut c_void,
    pub is_oson: c_int,
    pub vector_dimensions: u32,
    pub vector_format: u8,
    pub vector_flags: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct QueryInfo {
    pub name: *const c_char,
    pub name_length: u32,
    pub type_info: DataTypeInfo,
    pub null_ok: c_int,
}

// Layout guards (LP64). These catch FFI/struct drift at compile time.
const _: () = assert!(std::mem::size_of::<Bytes>() == 24);
const _: () = assert!(std::mem::size_of::<DataBuffer>() == 40);
const _: () = assert!(std::mem::size_of::<Data>() == 48);
const _: () = assert!(std::mem::align_of::<Data>() == 8);

#[repr(C)]
pub struct Context {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Conn {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Stmt {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Var {
    _private: [u8; 0],
}

unsafe impl Send for Context {}
unsafe impl Send for Conn {}
unsafe impl Send for Stmt {}
unsafe impl Send for Var {}

unsafe extern "C" {
    pub fn dpiContext_createWithParams(
        major_version: c_uint,
        minor_version: c_uint,
        create_params: *const c_void,
        context: *mut *mut Context,
        error: *mut ErrorInfo,
    ) -> c_int;
    pub fn dpiContext_destroy(context: *const Context);
    pub fn dpiContext_getError(context: *const Context, error: *mut ErrorInfo);

    pub fn dpiConn_create(
        context: *const Context,
        user_name: *const c_char,
        user_name_length: u32,
        password: *const c_char,
        password_length: u32,
        connect_string: *const c_char,
        connect_string_length: u32,
        common_create_params: *const c_void,
        create_params: *const c_void,
        connection: *mut *mut Conn,
    ) -> c_int;
    pub fn dpiConn_release(connection: *mut Conn) -> c_int;
    pub fn dpiConn_commit(connection: *mut Conn) -> c_int;
    pub fn dpiConn_rollback(connection: *mut Conn) -> c_int;
    pub fn dpiConn_prepareStmt(
        connection: *mut Conn,
        scrollable: c_int,
        sql_text: *const c_char,
        sql_text_length: u32,
        tag: *const c_char,
        tag_length: u32,
        statement: *mut *mut Stmt,
    ) -> c_int;
    #[allow(clippy::too_many_arguments)]
    pub fn dpiConn_newVar(
        connection: *mut Conn,
        oracle_type_num: u32,
        native_type_num: u32,
        max_array_size: u32,
        size: u32,
        size_is_bytes: c_int,
        is_array: c_int,
        obj_type: *mut c_void,
        var: *mut *mut Var,
        value: *mut *mut Data,
    ) -> c_int;

    pub fn dpiStmt_execute(
        statement: *mut Stmt,
        mode: u32,
        num_query_columns: *mut u32,
    ) -> c_int;
    pub fn dpiStmt_getQueryInfo(statement: *mut Stmt, pos: u32, info: *mut QueryInfo) -> c_int;
    pub fn dpiStmt_define(statement: *mut Stmt, pos: u32, var: *mut Var) -> c_int;
    pub fn dpiStmt_fetchRows(
        statement: *mut Stmt,
        max_rows: u32,
        buffer_row_index: *mut u32,
        num_rows_fetched: *mut u32,
        more_rows: *mut c_int,
    ) -> c_int;
    pub fn dpiStmt_getBindCount(statement: *mut Stmt, count: *mut u32) -> c_int;
    pub fn dpiStmt_getBindNames(
        statement: *mut Stmt,
        num_bind_names: *mut u32,
        bind_names: *mut *const c_char,
        bind_name_lengths: *mut u32,
    ) -> c_int;
    pub fn dpiStmt_bindByName(
        statement: *mut Stmt,
        name: *const c_char,
        name_length: u32,
        var: *mut Var,
    ) -> c_int;
    pub fn dpiStmt_getRowCount(statement: *mut Stmt, count: *mut u64) -> c_int;
    pub fn dpiStmt_release(statement: *mut Stmt) -> c_int;

    pub fn dpiVar_setFromBytes(
        var: *mut Var,
        pos: u32,
        value: *const c_char,
        value_length: u32,
    ) -> c_int;
    pub fn dpiVar_release(var: *mut Var) -> c_int;
}

/// RAII context handle.
pub struct ContextHandle(pub *mut Context);

impl ContextHandle {
    pub fn new() -> Self {
        let mut ctx: *mut Context = std::ptr::null_mut();
        let mut err = ErrorInfo::default();
        let (major, minor) = dpi_version();
        let rc = unsafe {
            dpiContext_createWithParams(major, minor, std::ptr::null(), &mut ctx, &mut err)
        };
        if rc != 0 {
            let msg = unsafe {
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    err.message as *const u8,
                    err.message_length as usize,
                ))
            };
            eprintln!("chcsv: context create Error, Code={}:{}", err.code, msg);
            std::process::exit(1);
        }
        ContextHandle(ctx)
    }

    /// Fetch the last ODPI error and terminate like the original OraErr():
    /// print "chcsv: <what> Error, Code=<code>:<message>" and exit(code).
    pub fn fail(&self, what: &str, rc: c_int) -> ! {
        let mut err = ErrorInfo::default();
        unsafe { dpiContext_getError(self.0, &mut err) };
        let code = if err.code != 0 { err.code } else { rc };
        let msg = unsafe {
            String::from_utf8_lossy(std::slice::from_raw_parts(
                err.message as *const u8,
                err.message_length as usize,
            ))
        };
        // The original rolls back and releases before exiting; ODPI does it on drop.
        eprintln!("chcsv: {what} Error, Code={code}:{msg}");
        std::process::exit(code)
    }
}

impl Drop for ContextHandle {
    fn drop(&mut self) {
        unsafe { dpiContext_destroy(self.0) };
    }
}
