#![no_main]


use serde::{Deserialize, Serialize};

use serde_json::{Value, json};
use zflow_plugin::{*, extension::*};

#[repr(C)]
#[derive(Serialize, Deserialize)]
pub struct Output {
    pub result: i64,
}


#[repr(C)]
#[derive(Serialize, Deserialize, Debug)]
pub struct Input {
    pub a: Value,
    pub b: Value,
}

pub fn add(input: Value) -> FnResult<()> {
    // because inputs are controlled, we wait for all of them
    let input = Input::deserialize(input)?;
    if input.a != Value::Null && input.b != Value::Null {
        let left = input.a.as_i64().unwrap();
        let right = input.b.as_i64().unwrap();

        let data = Output {
            result: left + right,
        };
        unsafe {
            // send output to host
            // send(Json(data))?;
            send_done(Json(data))?;
            // send_buffer("Out".to_owned(), &[1, 2, 4])?;
        }
    }

    return Ok(());
}


pub fn sub(input: Value) -> FnResult<()> {
    let input = Input::deserialize(input)?;
    if input.a != Value::Null && input.b != Value::Null {
        let left = input.a.as_i64().unwrap();
        let right = input.b.as_i64().unwrap();

        let data = Output {
            result: left - right,
        };
        unsafe {
            // send output to host
            send_done(Json(data))?;
        }
    }

    return Ok(());
}


#[plugin_fn]
pub unsafe fn provider_id() -> FnResult<String> {
    return Ok("@test/provider".to_owned())
}

#[plugin_fn]
pub unsafe fn get_logo() -> FnResult<String> {
    // return valid SVG logo icon
    return Ok("".to_owned())
}

#[plugin_fn]
pub unsafe fn get_platform() -> FnResult<Json<Platform>> {
    return Ok(Json(Platform::System))
}

#[plugin_fn]
pub unsafe fn get_packages() -> FnResult<Json<Vec<Package>>> {
    let packages = json!([
        {
            "package_id": "math",
            "components": [
                {
                    "name": "math/add",
                    "inports": {
                        "a": {
                            "trigerring": true,
                            "control": true
                        },
                        "b": {
                            "trigerring": true,
                            "control": true
                        }
                    },
                    "outports": {
                        "result": {}
                    },
                    "description": "Addition of two numbers",
                    "ordered": true
                },
                {
                    "name": "math/sub",
                    "inports": {
                        "input": {
                            "trigerring": true,
                            "control": true
                        }
                    },
                    "outports": {
                        "result": {}
                    },
                    "description": "Substraction of two numbers",
                    "ordered": true
                }
            ]
        }
    ]);

    let packages = packages.as_array().unwrap().iter().map(|p| Package::deserialize(p).unwrap()).collect::<Vec<Package>>();

    return Ok(Json(packages));
}


#[plugin_fn]
pub unsafe fn run_component(packet:Json<ComponentWithInput>) -> FnResult<()> {
    let packet = packet.into_inner();
    match packet.component.name.as_str() {
        "math/add"=> {
            add(packet.input)?;
        },
        "math/sub"=> {
            sub(packet.input)?;
        },
        _ => {
            return Err(Error::msg("Unknown process").into())
        }
    }

    Ok(())
}