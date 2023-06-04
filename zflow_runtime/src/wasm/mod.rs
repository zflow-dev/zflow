use std::{collections::HashMap, path::PathBuf};

use extism::{
    manifest::Wasm, Context, CurrentPlugin, Function, Manifest, Plugin, UserData, Val, ValType,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{
    component::{Component, ComponentOptions, ModuleComponent},
    ip::IPType,
    port::{InPort, OutPort, PortOptions},
    process::{ProcessError, ProcessResult},
};

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct WasmComponent {
    pub name: String,
    pub inports: HashMap<String, PortOptions>,
    pub outports: HashMap<String, PortOptions>,
    #[serde(default)]
    /// Set the default component description
    pub description: String,
    #[serde(default)]
    /// Set the default component icon
    pub icon: String,
    #[serde(default)]
    /// Whether the component should keep send packets
    /// out in the order they were received
    pub ordered: bool,
    #[serde(default)]
    /// Whether the component should activate when it receives packets
    pub activate_on_input: bool,
    #[serde(default)]
    /// Bracket forwarding rules. By default we forward
    pub forward_brackets: HashMap<String, Vec<String>>,
    /// Base directory of wasm sources
    pub base_dir: String,
    /// Path to wasm source
    pub source: String,
    #[serde(default)]
    pub package_id: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl WasmComponent {
    pub fn from_metadata(meta: Value) -> Option<WasmComponent> {
        WasmComponent::deserialize(meta).ok()
    }

    pub fn with_metadata(&mut self, meta: Value) -> WasmComponent {
        if let Some(meta) = WasmComponent::from_metadata(meta.clone()) {
            self.inports.extend(meta.inports);
            self.outports.extend(meta.outports);
            if !meta.description.is_empty() {
                self.description = meta.description;
            }
            if !meta.icon.is_empty() {
                self.icon = meta.icon;
            }
            self.forward_brackets.extend(meta.forward_brackets);
            if !meta.base_dir.is_empty() {
                self.base_dir = meta.base_dir;
            }
        } else if let Some(meta) = meta.clone().as_object() {
            self.metadata = meta.clone();
        }

        self.clone()
    }
}

impl ModuleComponent for WasmComponent {
    fn as_component(&self) -> Result<Component, String> {
        let source = self.source.clone();
        let mut code = PathBuf::from(self.base_dir.clone());
        code.push(source);
        let code = code.as_os_str();

        if let Some(source_code) = code.to_str() {
            let manifest = Manifest::new([Wasm::file(source_code.clone().to_string())]);
            let mut inports = self.inports.clone();
            let mut outports = self.outports.clone();
            if inports.is_empty() {
                inports.insert("in".to_owned(), PortOptions::default());
            }
            if outports.is_empty() {
                outports.insert("out".to_owned(), PortOptions::default());
            }

            return Ok(Component::new(ComponentOptions {
                metadata: Some(self.metadata.clone()),
                in_ports: HashMap::from_iter(
                    inports
                        .clone()
                        .iter()
                        .map(|(key, options)| (key.clone(), InPort::new(options.clone())))
                        .collect::<Vec<_>>(),
                ),
                out_ports: HashMap::from_iter(
                    outports
                        .clone()
                        .iter()
                        .map(|(key, options)| (key.clone(), OutPort::new(options.clone())))
                        .collect::<Vec<_>>(),
                ),
                description: self.description.clone(),
                icon: self.icon.clone(),
                ordered: self.ordered,
                activate_on_input: self.activate_on_input,
                forward_brackets: self.forward_brackets.clone(),
                graph: None,
                process: Some(Box::new(move |handle| {
                    let inputs: Vec<&String> = inports.keys().collect();
                    if let Ok(this) = handle.clone().try_lock().as_mut() {
                        let _inputs: HashMap<&String, Value> = HashMap::from_iter(
                            inputs
                                .clone()
                                .iter()
                                .map(|port| {
                                    let value = this.input().get(*port);
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

                        let output = this.output();

                        // `send` Host function for use in the wasm binary
                        let send_fn = Function::new(
                            "send",
                            [ValType::I64],
                            [ValType::I64],
                            None,
                            move |_plugin: &mut CurrentPlugin,
                                  params: &[Val],
                                  returns: &mut [Val],
                                  _: UserData| {
                                let data = _plugin.memory.get(params[0].unwrap_i64() as usize)?;

                                if let Ok(result) =
                                    serde_json::from_str::<Value>(std::str::from_utf8(data).expect(
                                        "expected to decode output value from wasm component",
                                    ))
                                {
                                    if let Some(res) = result.as_object() {
                                        println!("wasm-output => {:?}", res);
                                        output.clone().send(res).expect("expected output value");
                                    }
                                }
                                returns[0] = params[0].clone();

                                Ok(())
                            },
                        );

                        let output = this.output();
                        let send_buffer_fn = Function::new(
                            "send_buffer",
                            [ValType::I64],
                            [ValType::I64],
                            None,
                            move |_plugin: &mut CurrentPlugin,
                                  params: &[Val],
                                  returns: &mut [Val],
                                  _: UserData| {
                                let port = _plugin.memory.get(params[0].unwrap_i64() as usize)?;
                                let data = _plugin.memory.get(params[1].unwrap_i64() as usize)?;

                                if let Ok(port) = std::str::from_utf8(port) {
                                    output
                                        .clone()
                                        .send_buffer(port, data)
                                        .expect("expected output value");
                                }
                                returns[0] = params[0].clone();

                                Ok(())
                            },
                        );

                        let output = this.output();
                        // `send_done` Host function for use in the wasm binary
                        let send_done_fn = Function::new(
                            "send_done",
                            [ValType::I64],
                            [ValType::I64],
                            None,
                            move |_plugin: &mut CurrentPlugin,
                                  params: &[Val],
                                  returns: &mut [Val],
                                  _: UserData| {
                                let data = _plugin.memory.get(params[0].unwrap_i64() as usize)?;

                                if let Ok(result) =
                                    serde_json::from_str::<Value>(std::str::from_utf8(data).expect(
                                        "expected to decode output value from wasm component",
                                    ))
                                {
                                    if let Some(res) = result.as_object() {
                                        if let Err(x) = output.clone().send_done(res) {
                                            output
                                                .clone()
                                                .send_done(&x)
                                                .expect("expected to send error");
                                        }
                                    }
                                }
                                returns[0] = params[0].clone();

                                Ok(())
                            },
                        );

                        let context = Context::new();
                        let plugin = Plugin::new_with_manifest(
                            &context,
                            &manifest,
                            [&send_fn, &send_done_fn],
                            false,
                        );

                        if plugin.is_err() {
                            let err = plugin.err().unwrap().to_string();
                            return Err(ProcessError(err));
                        }

                        let mut plugin = plugin.unwrap();

                        let x =
                            plugin.call("process", serde_json::to_string(&mapped_inputs).unwrap());
                        if x.is_err() {
                            return Err(ProcessError(format!(
                                "Failed to call main function from wasm component: {}",
                                x.err().unwrap().to_string()
                            )));
                        }
                        if let Ok(result) = serde_json::from_str::<Value>(
                            std::str::from_utf8(x.unwrap())
                                .expect("expected to decode return value from wasm component"),
                        ) {
                            if let Some(res) = result.as_object() {
                                this.output().send(res)?;
                            }
                        }
                    }
                    Ok(ProcessResult::default())
                })),
            }));
        }
        Err(format!("Could not load wasm component"))
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use zflow_graph::Graph;

    use crate::network::{Network, NetworkOptions, BaseNetwork};

    use super::*;

    #[test]
    fn create_wasm_component() {
        let mut base_dir = std::env::current_dir().unwrap();
        base_dir.push("test_components");
        let base_dir = base_dir.to_str().unwrap();

        let mut graph = Graph::new("wasm_graph", false);
        graph.add_node("zflow", "add_wasm", None)
            .add_initial(json!(1), "zflow", "left", None)
            .add_initial(json!(2), "zflow", "right", None);

        let mut network = Network::create(
            graph.clone(),
            NetworkOptions {
                subscribe_graph: false,
                delay: true,
                base_dir: base_dir.to_string(),
                ..Default::default()
            },
        );

        if let Ok(nw) = network.connect().unwrap().try_lock().as_mut() {
            nw.start().unwrap();
            thread::sleep(Duration::from_secs(2));
        }
    }
}
