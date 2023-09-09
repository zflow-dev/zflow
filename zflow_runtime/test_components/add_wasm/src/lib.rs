#![no_main]

use extism_pdk::{json::Value, *};
use serde::{Deserialize, Serialize};


#[host_fn]
extern "ExtismHost" {
    fn send(output: Json<Output>)-> Json<Output>;
    fn send_done(output: Json<Output>)-> Json<Output>;
}


#[repr(C)]
#[derive(Serialize, Deserialize)]
pub struct Output {
    pub sum: i64,
}

impl Output {
    #[inline(always)]
    pub fn as_ptr(&self) -> u64 {
        let mem = extism_pdk::Memory::from_bytes(json::to_string(self.clone()).unwrap().as_bytes());
        mem.keep().offset
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Packet {
    pub input: Input,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Input {
    pub left: Value,
    pub right: Value,
}

#[plugin_fn]
pub fn process(packet: Json<Packet>) -> FnResult<Json<Output>> {
    // because inputs are controlled, we wait for all of them
    let input:Input = packet.0.input;
    if input.left != Value::Null && input.right != Value::Null {
        let left = input.left.as_i64().unwrap();
        let right = input.right.as_i64().unwrap();

        let data = Output { sum: left + right };
        unsafe {
            // send output to host
            send_done(Json(data))?;
        }
    }

    return Ok(Json(Output { sum: 0 }));
}
