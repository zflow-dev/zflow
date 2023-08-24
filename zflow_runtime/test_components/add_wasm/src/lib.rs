#![no_main]

use extism_pdk::{json::Value, *};
use serde::{Deserialize, Serialize};

extern "C" {
    fn send(output: u64) -> u64;
    fn send_done(output: u64) -> u64;
}

#[derive(Serialize, Deserialize)]
pub struct Output {
    pub sum: i64,
}

impl Output {
    #[cfg(not(tarpaulin_include))]
    #[inline(always)]
    pub fn as_ptr(&self) -> u64 {
        let mem = extism_pdk::Memory::from_bytes(json::to_string(self.clone()).unwrap().as_bytes());
        mem.keep().offset
    }
}

#[derive(Serialize, Deserialize)]
pub struct Input {
    pub left: Value,
    pub right: Value,
}

#[plugin_fn]
pub fn process(input: Json<Input>) -> FnResult<Json<Value>> {
    // because inputs are controlled, we wait for all of them
    if input.0.left == Value::Null && input.0.right == Value::Null {
        return Ok(Json(Value::Null));
    }
    let left = input.0.left.as_i64().unwrap();
    let right = input.0.right.as_i64().unwrap();

    let data_ptr = Output { sum: left + right }.as_ptr();
    unsafe {
        // send output to host
        send(data_ptr);
    }

    return Ok(Json(Value::Null));
}
