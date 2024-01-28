use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use extism::{Function, Manifest, Plugin, UserData, ValType, WasmInput};
use serde_json::{json, Value};
use zflow_plugin::{ComponentSource, OutputBuffer, ComponentWithInput, Platform};

use crate::{
    ip::IPType,
    process::{ProcessError, ProcessResult},
    provider::ProviderRunner,
    runner::{RunFunc, WASM_RUNNER_ID},
};

pub fn get_sys_wasm_runner<'a>(
    component: ComponentSource,
    process_name: &'a str,
    manifest: Manifest,
    is_provider: bool,
) -> Result<ProviderRunner, anyhow::Error> {
    let id = component.name.to_owned();
    let process_name = process_name.to_owned();

    let runner_func: Box<RunFunc> = Box::new(move |handle| {
        let handle_binding = handle.clone();
        let mut handle_binding = handle_binding.try_lock();
        let this = handle_binding
            .as_mut()
            .map_err(|_| ProcessError(String::from("Process Handle has dropped")))?;

        let inports = this.input().in_ports.ports;

        let inport_keys = inports.keys();
        let controlled_data = this
            .input()
            .in_ports
            .ports
            .iter()
            .filter(|(_, port)| port.options.control)
            .map(|(key, _)| this.input().get(key))
            .collect::<Vec<_>>();

        if !controlled_data.is_empty() && controlled_data.contains(&None) {
            return Ok(ProcessResult::default());
        }

        let _inputs: HashMap<String, Value> = HashMap::from_iter(
            inport_keys
                .map(|port| {
                    let value = this.input().get(port);
                    if let Some(value) = value {
                        return (
                            port.clone(),
                            match value.datatype {
                                IPType::Data(v) => v,
                                _ => Value::Null,
                            },
                        );
                    }
                    return (port.clone(), Value::Null);
                })
                .collect::<Vec<_>>(),
        );

        let mapped_inputs = json!(_inputs);

        let _id = id.clone();

        let send_done_fn = Function::new(
            "send_done",
            [ValType::I64],
            [],
            UserData::Rust(Arc::new(Mutex::new(this.output()))),
            move |plugin, params, _, userdata| {
                let _output = &params[0];
                let handle = plugin.memory_handle(_output.unwrap_i64() as u64).unwrap();
                let bytes = plugin.memory_str(handle)?;
                let data: Value = serde_json::from_str(bytes)?;

                let output = userdata.get()?.clone();
                output.lock().unwrap().send_done(&data).expect(&format!(
                    "expected to send output data from component {}\n",
                    _id.clone()
                ));
                Ok(())
            },
        );

        let _id = id.clone();

        let send_fn = Function::new(
            "send",
            [ValType::I64],
            [],
            UserData::Rust(Arc::new(Mutex::new(this.output()))),
            move |plugin, params, _, userdata| {
                let _output = &params[0];
                let handle = plugin.memory_handle(_output.unwrap_i64() as u64).unwrap();
                let bytes = plugin.memory_str(handle)?;
                let data: Value = serde_json::from_str(bytes)?;
                let output = userdata.get()?.clone();
                output.lock().unwrap().send(&data).expect(&format!(
                    "expected to send output data from component {}\n",
                    _id.clone()
                ));
                Ok(())
            },
        );

        let _id = id.clone();

        // `send_buffer` Host function for use in the wasm binary
        let send_buffer_fn = Function::new(
            "send_buffer",
            [ValType::I64],
            [ValType::I64],
            UserData::Rust(Arc::new(Mutex::new(this.output()))),
            move |plugin, params, _, userdata| {
                let handle = plugin.memory_from_val(&params[0]).unwrap();
                let data =
                    serde_json::from_str::<OutputBuffer>(plugin.memory_str(handle)?).unwrap();
                let output = userdata.get()?.clone();
                output
                    .lock()
                    .unwrap()
                    .send_buffer(data.port, data.packet)
                    .expect(&format!(
                        "expected to send output data from component {}\n",
                        _id.clone()
                    ));

                Ok(())
            },
        );

        let plugin = Plugin::new(
            WasmInput::Manifest(manifest.clone()),
            [send_done_fn, send_fn, send_buffer_fn],
            true,
        );
        if plugin.is_err() {
            let err = plugin.err().unwrap().to_string();
            return Err(ProcessError(err));
        }
        let mut plugin = plugin.unwrap();

        let call_input = if is_provider {
            json!(ComponentWithInput {
                component: component.clone(),
                input: mapped_inputs
            })
        } else {
            mapped_inputs
        };

        let data = plugin.call(
            process_name.clone(),
            serde_json::to_string(&call_input).unwrap(),
        );

        if data.is_err() {
            return Err(ProcessError(format!(
                "Failed to call main function from wasm component: {}",
                data.err().unwrap().to_string()
            )));
        }
        if let Ok(result) = serde_json::from_str::<Value>(
            std::str::from_utf8(data.unwrap())
                .expect("expected to decode return value from wasm component"),
        ) {
            if let Some(res) = result.as_object() {
                return Ok(ProcessResult {
                    data: json!(res.clone()),
                    resolved: true,
                    ..ProcessResult::default()
                });
            }
        }

        Ok(ProcessResult::default())
    });

    return Ok(ProviderRunner {
        runner_id: WASM_RUNNER_ID.to_owned(),
        runner_func,
        platforms: vec![Platform::System],
    });
}
