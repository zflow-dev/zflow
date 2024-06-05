#![no_main]

use json::Value;
use zflow_plugin::{*, extension::*};
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Serialize, Deserialize)]
pub struct Output {
    pub sum: i64,
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
