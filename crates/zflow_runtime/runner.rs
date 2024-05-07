use std::sync::{Arc, Mutex};


use crate::process::{ProcessError, ProcessHandle, ProcessResult};

pub type RunFunc = dyn FnMut(Arc<Mutex<ProcessHandle>>) -> Result<ProcessResult, ProcessError>
    + Send
    + Sync
    + 'static;

pub type DynRunFunc = dyn FnMut(Box<[u8]>, Arc<Mutex<ProcessHandle>>) -> Result<ProcessResult, ProcessError>
    + Send
    + Sync
    + 'static;


pub type UnsafeRunFunc = unsafe extern fn(
    source: *const u8,
    length: usize,
    handle: *const Arc<Mutex<ProcessHandle>>,
    result: *mut Result<ProcessResult, ProcessError>
);

pub(crate) const WASM_RUNNER_ID: &str = "@zflow/wasm";
pub(crate) const DENO_RUNNER_ID: &str = "@zflow/deno";

pub(crate) const QUICKJS_RUNNER_ID: &str = "@zflow/quickjs";


